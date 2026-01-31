# Architecture

## System Overview

PMHFT is a Cargo workspace with 11 crates, structured as a pipeline:

```
Market Data → Normalization → Arb Detection → Risk Gating → Execution → Position Tracking
```

All subsystems run as independent tokio tasks, communicating via broadcast and mpsc channels. The application orchestrator (`src/app.rs`) wires everything together and monitors for task failures.

## Data Flow

```
┌─────────────────────┐     ┌─────────────────────┐
│  Dome WebSocket     │     │  Dome REST (1/s)     │
│  (Polymarket live)  │     │  (Kalshi polling)    │
└────────┬────────────┘     └────────┬─────────────┘
         │                           │
         ▼                           ▼
┌─────────────────────┐     ┌─────────────────────┐
│  PolymarketFeed     │     │  KalshiFeed          │
│  WS msg → Quote     │     │  REST → Quote        │
└────────┬────────────┘     └────────┬─────────────┘
         │                           │
         └──────────┬────────────────┘
                    │  broadcast::Sender<NormalizedQuote>
                    ▼
         ┌──────────────────────┐
         │ MarketDataAggregator │
         │ (book cache, dedup)  │
         └──────────┬───────────┘
                    │  broadcast::Sender<MarketDataEvent>
                    ▼
         ┌──────────────────────┐
         │    ArbDetector       │
         │ (direct/comp/stat)   │
         └──────────┬───────────┘
                    │  mpsc::Sender<ArbSignal>
                    ▼
         ┌──────────────────────┐
         │   ExecutionEngine    │
         │ ┌──────┐ ┌────────┐ │
         │ │ Risk │→│Execute │ │
         │ │Check │ │Both    │ │
         │ └──────┘ │Legs    │ │
         │          └───┬────┘ │
         └──────────────┼──────┘
                ┌───────┴───────┐
                ▼               ▼
         ┌────────────┐  ┌────────────┐
         │ Dome REST  │  │ Kalshi FIX │
         │ placeOrder │  │ NewOrder   │
         │(Polymarket)│  │ (or REST)  │
         └────────────┘  └────────────┘
```

## Crate Dependency Graph

```
pmhft-common            (no internal deps)
    ▲
    │
    ├── pmhft-telemetry         (no internal deps)
    │
    ├── pmhft-orderbook
    │
    ├── pmhft-dome-client
    │
    ├── pmhft-kalshi-client
    │
    ├── pmhft-polymarket-signer
    │
    ├── pmhft-matching ──────────── pmhft-dome-client
    │
    ├── pmhft-market-data ───────── pmhft-dome-client
    │   │                           pmhft-kalshi-client
    │   │                           pmhft-orderbook
    │   └──────────────────────────  pmhft-matching
    │
    ├── pmhft-strategy ──────────── pmhft-orderbook
    │   └──────────────────────────  pmhft-matching
    │
    ├── pmhft-risk
    │
    └── pmhft-execution ─────────── pmhft-dome-client
        │                           pmhft-kalshi-client
        │                           pmhft-polymarket-signer
        │                           pmhft-risk
        └──────────────────────────  pmhft-telemetry
```

## Key Design Decisions

### Channel Architecture

| Channel | Type | Buffer | Purpose |
|---|---|---|---|
| `quote_tx` | `broadcast<NormalizedQuote>` | 8192 | Market data distribution to all consumers |
| `unified_rx` | `broadcast<MarketDataEvent>` | 4096 | Aggregated events for strategy consumption |
| `signal_tx` | `mpsc<ArbSignal>` | 256 | Arb signals from detector to execution engine |
| `dome_ws_rx` | `broadcast<DomeWsMessage>` | 8192 | Raw WebSocket events from Dome |

Broadcast channels are used for market data because multiple consumers (aggregator, strategy, telemetry) need the same data. MPSC is used for arb signals because there's exactly one producer (detector) and one consumer (execution engine).

### Decimal Arithmetic

All prices and quantities use `rust_decimal::Decimal` rather than floating point. This prevents rounding errors in fee calculations and edge computations where a fraction of a cent matters.

### Concurrency Model

- **tokio** async runtime for all I/O-bound work (network, timers)
- **DashMap** for lock-free concurrent state (pair registry, orderbook cache, spread trackers)
- **Atomic operations** for counters (position tracker, open orders, circuit breaker)
- No dedicated CPU-pinned threads in the current implementation -- the hot path (market data to signal emission) runs on the tokio thread pool

### Rate Limiting

The Dome free tier allows 1 request/second. The rate limiter (`pmhft-dome-client/src/rest/rate_limit.rs`) uses a tokio semaphore with a background refill task to enforce this across all REST endpoints. Strategy:

- Polymarket data comes via WebSocket (no REST budget consumed)
- Kalshi data requires REST polling -- the `KalshiFeed` round-robins through active pairs, one per tick interval
- Order placement (Dome `placeOrder`) shares the same rate limit pool
- Market discovery runs infrequently (every 15-30 minutes)

### Execution Model

Both legs execute concurrently via `tokio::join!`:

```
ArbSignal arrives
    ├── Check signal TTL (reject if expired)
    ├── Pre-trade risk check (5 validations)
    ├── tokio::join!(poly_executor.execute, kalshi_executor.execute)
    └── Handle results:
        ├── Both filled    → update positions, log PnL
        ├── One filled     → unwind the filled leg immediately
        └── Both failed    → log, no action
```

The Kalshi executor prefers FIX 4.4 (`NewOrderSingle`) when a FIX session is configured, falling back to REST (`POST /orders`). The Polymarket executor always uses Dome's `placeOrder` JSON-RPC endpoint.

### Risk Controls

Pre-trade checks run synchronously before every execution:

1. **Circuit breaker** -- global kill switch, tripped by daily loss limit
2. **Position per market** -- max contracts held in any single market (default: 500)
3. **Gross exposure** -- total dollar value across all positions (default: $10,000)
4. **Order notional** -- per-order size limit (default: $1,000)
5. **Open orders** -- concurrent order limit (default: 20)

Post-trade: positions and realized PnL are updated atomically. If daily loss exceeds the threshold, the circuit breaker trips and halts all trading.

## Authentication

### Polymarket (EIP-712)

Orders are signed using EIP-712 typed data signatures on Polygon (chain ID 137). The `pmhft-polymarket-signer` crate handles:

- EIP-712 domain construction (exchange contract address, chain ID)
- Order struct hashing per Polymarket's CLOB specification
- ECDSA signing via `alloy-signer-local`
- HMAC-SHA256 for L2 CLOB REST API authentication headers

### Kalshi (RSA-PSS)

Kalshi uses RSA-PSS with SHA-256:

- REST: `message = timestamp_ms + method + path` → RSA-PSS sign → base64 → `KALSHI-ACCESS-SIGNATURE` header
- FIX: RSA-PSS signed logon message with API key as SenderCompID

### Dome

API key passed as query parameter or header on all REST requests, and as path component for WebSocket URL.

## FIX 4.4 Protocol (Kalshi)

The FIX implementation (`pmhft-kalshi-client/src/fix/`) handles:

- **Session layer**: Logon (35=A), Heartbeat (35=0), TestRequest (35=1), Logout (35=5)
- **Application messages**: NewOrderSingle (35=D), OrderCancelRequest (35=F), ExecutionReport (35=8)
- **Sequence management**: Monotonically increasing message sequence numbers per direction
- **Connection**: TCP + TLS via `tokio-rustls`
- **Codec**: Custom FIX tag=value parser with SOH delimiter, checksum validation

The FIX session manager runs as a background task, dispatching incoming ExecutionReports via a broadcast channel that the `KalshiExecutor` subscribes to.

## Pair Matching Pipeline

```
Every 15 min (sports):
    Dome API /matching-markets/sports/{sport}
    → 7 sports: NFL, NBA, MLB, NHL, MLS, NCAAF, NCAAB
    → Direct cross-platform pair mappings
    → Upsert into PairRegistry

Every 30 min (all categories):
    Dome API /polymarket/markets + /kalshi/markets
    → Normalize titles (lowercase, strip punctuation, remove stopwords)
    → Group by inferred category (politics, crypto, weather, ...)
    → Jaro-Winkler similarity + keyword overlap scoring
    → Accept matches above threshold (default: 0.85)
    → Upsert into PairRegistry
```

The `PairRegistry` maintains reverse indexes (Polymarket token ID → pair, Kalshi ticker → pair) for O(1) lookup when processing market data events.

## Statistical Arbitrage Details

The stat arb module (`pmhft-strategy/src/spread_tracker.rs`) maintains a rolling window of cross-platform spread observations:

```
spread(t) = poly_mid(t) - kalshi_mid(t)
```

**Z-score**: `z = (spread - mean) / std_dev` over the configured window (default: 100 ticks).

**Half-life estimation**: Fits an AR(1) model via OLS on consecutive spread differences. If the autoregressive coefficient implies a half-life longer than the configured maximum (default: 1 hour), the pair is skipped -- it won't mean-revert fast enough to be profitable.

**Entry/exit**:
- Entry when `|z| > z_score_entry` (default: 2.0)
- Exit when `|z| < z_score_exit` (default: 0.5)
- Direction: if z > 0 (Poly overpriced), sell Poly + buy Kalshi; if z < 0, reverse

## Paper Trading Mode

When `execution.live_trading = false` (the default), both executors simulate fills:

- Orders are logged with `[PAPER]` prefix
- Fill price equals the limit price (no slippage simulation)
- Fees are computed at standard rates (Polymarket ~2%, Kalshi ~7 cents/contract)
- Position tracking, risk checks, and PnL accounting all operate normally
- No real orders are sent to any exchange

This allows end-to-end validation of the signal pipeline against live market data without financial risk.
