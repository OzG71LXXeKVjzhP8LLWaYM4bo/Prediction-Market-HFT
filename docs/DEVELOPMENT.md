# Development Guide

## Build Commands

```bash
# Check compilation (fast, no codegen)
cargo check --workspace

# Build debug
cargo build --workspace

# Build release (LTO, stripped)
cargo build --release

# Run all tests
cargo test --workspace

# Run tests for a single crate
cargo test -p pmhft-orderbook
cargo test -p pmhft-matching
cargo test -p pmhft-strategy
cargo test -p pmhft-kalshi-client
```

## Workspace Structure

The project is a Cargo workspace with 11 internal crates plus a root binary. Dependencies are declared in the workspace `[workspace.dependencies]` section and inherited by member crates.

### Crate Overview

| Crate | Purpose | Internal Dependencies |
|---|---|---|
| `pmhft-common` | Domain types, errors, config | None |
| `pmhft-telemetry` | Logging + Prometheus metrics | None |
| `pmhft-orderbook` | L2 orderbook data structure | `pmhft-common` |
| `pmhft-dome-client` | Dome REST + WebSocket client | `pmhft-common` |
| `pmhft-kalshi-client` | Kalshi REST + FIX 4.4 client | `pmhft-common` |
| `pmhft-polymarket-signer` | EIP-712 + HMAC signing | `pmhft-common` |
| `pmhft-matching` | Cross-platform pair discovery | `pmhft-common`, `pmhft-dome-client` |
| `pmhft-market-data` | Feed aggregation | `pmhft-common`, `pmhft-orderbook`, `pmhft-dome-client`, `pmhft-kalshi-client`, `pmhft-matching` |
| `pmhft-strategy` | Arb detection | `pmhft-common`, `pmhft-orderbook`, `pmhft-matching` |
| `pmhft-execution` | Dual-leg execution | `pmhft-common`, `pmhft-dome-client`, `pmhft-kalshi-client`, `pmhft-polymarket-signer`, `pmhft-risk`, `pmhft-telemetry` |
| `pmhft-risk` | Risk management | `pmhft-common` |

---

## Crate Details

### pmhft-common

Location: `crates/pmhft-common/src/`

Core shared types used by every other crate. Changing types here affects the entire workspace.

**Key files:**
- `types.rs` -- `Platform`, `OutcomeSide`, `Direction`, `MarketId`, `MatchedPair`, `NormalizedQuote`, `PriceLevel`, `ArbSignal`, `LegInstruction`, `FillReport`, `Order`
- `error.rs` -- `PmhftError` enum with variants for every failure mode (API errors, signing errors, risk limit breaches, etc.). All crates use `pmhft_common::Result<T>`.
- `config.rs` -- `AppConfig` and nested section structs (`DomeConfig`, `KalshiConfig`, etc.). All implement `Default` and `serde::Deserialize`.

Notable: `Direction` has an `opposite()` method used by the execution engine for unwind logic.

### pmhft-telemetry

Location: `crates/pmhft-telemetry/src/lib.rs`

Single-file crate. Initializes `tracing-subscriber` for structured logging and `metrics-exporter-prometheus` for the metrics HTTP endpoint.

Public functions:
- `init_logging(level, json)` -- sets up the global tracing subscriber
- `init_metrics(port)` -- starts the Prometheus exporter HTTP server
- `inc_counter(name)`, `set_gauge(name, value)`, `record_histogram(name, value)` -- metric recording helpers

### pmhft-orderbook

Location: `crates/pmhft-orderbook/src/book.rs`

BTreeMap-based L2 orderbook. Bids are stored in reverse order (highest first) using a `PriceKey` wrapper that implements `Ord` with reversed comparison for the bid side.

Key methods:
- `apply_snapshot(bids, asks, timestamp, sequence)` -- replaces entire book
- `apply_delta(bid_updates, ask_updates, timestamp, sequence)` -- incremental update (rejects stale updates by sequence number, removes levels with zero size)
- `best_bid()`, `best_ask()` -- BBO extraction
- `mid_price()`, `spread()` -- derived calculations
- `ask_liquidity_up_to(max_price)`, `bid_liquidity_down_to(min_price)` -- depth aggregation

**7 unit tests** covering snapshot apply, delta updates, stale rejection, zero-size filtering, and liquidity computation.

### pmhft-dome-client

Location: `crates/pmhft-dome-client/src/`

REST and WebSocket client for the Dome API.

**REST (`rest/`)**:
- `rate_limit.rs` -- Token bucket using `tokio::sync::Semaphore`. A background task adds one permit per `1/rate_limit_per_sec` seconds. All REST methods call `self.limiter.acquire().await` before making HTTP requests.
- `polymarket.rs` -- `DomePolymarketClient` with methods: `search_markets()`, `get_market()`, `get_market_price()`, `get_orderbooks()`, `place_order()`
- `kalshi.rs` -- `DomeKalshiClient` with methods: `get_markets()`, `get_market_price()`, `get_orderbook()`
- `matching.rs` -- `DomeMatchingClient` with `get_sports_matches(sport, date)`

**WebSocket (`ws/`)**:
- `connection.rs` -- `DomeWsConnection` handles connect, subscribe, auto-reconnect with exponential backoff (1s initial, 60s max), and ping/pong heartbeats. Subscriptions use Polymarket `condition_ids`.
- `handler.rs` -- `parse_order_event()` converts raw `DomeWsMessage` into `NormalizedQuote`.

**Types (`types.rs`)**: All Dome API request/response structures, including `DomeMarket`, `DomeKalshiOrderbook`, `DomePlaceOrderRequest` (JSON-RPC 2.0), `DomeWsMessage`, `DomeWsSubscription`.

### pmhft-kalshi-client

Location: `crates/pmhft-kalshi-client/src/`

**Authentication (`auth.rs`)**:
- `KalshiAuth` loads an RSA private key from PEM, signs requests using RSA-PSS with SHA-256
- `sign_request(method, path, body, timestamp)` produces a base64-encoded signature
- `apply_headers(builder, method, path)` adds `KALSHI-ACCESS-KEY`, `KALSHI-ACCESS-TIMESTAMP`, `KALSHI-ACCESS-SIGNATURE` headers

**REST (`rest/client.rs`)**:
- `KalshiRestClient` with `create_order()`, `cancel_order()`, `get_markets()`, `get_orderbook()`
- All requests are signed via `KalshiAuth`
- Configured against `demo-api.kalshi.co` by default

**FIX 4.4 (`fix/`)**:
- `codec.rs` -- `FixMessage` with `encode()` (SOH-delimited tag=value pairs, checksum) and `parse()`. Includes `tags` module with standard FIX tag constants. Has a round-trip encode/decode unit test.
- `messages.rs` -- Builder functions for FIX messages: `build_logon()`, `build_heartbeat()`, `build_new_order_single()`, `build_order_cancel_request()`. Also `ExecutionReport` struct with `from_fix()` parser.
- `connection.rs` -- `FixTcpConnection` establishes TCP+TLS via `tokio-rustls`, provides `send()` and `read()` methods
- `session.rs` -- `FixSession` manages the full FIX lifecycle: logon with RSA-PSS auth, heartbeat exchange, sequence number tracking, message dispatch. Provides `send_new_order()` and `send_cancel()` for the execution layer, plus `subscribe_exec_reports()` for fill notifications.

### pmhft-polymarket-signer

Location: `crates/pmhft-polymarket-signer/src/`

- `eip712.rs` -- Uses `alloy-sol-types` `sol!` macro to define the EIP-712 Order struct. `PolymarketOrderSigner` signs orders using a local wallet (`alloy-signer-local`). Domain is configured with chain ID and exchange contract address.
- `credentials.rs` -- `ClobCredentials` handles HMAC-SHA256 signing for Polymarket's L2 CLOB REST API. `sign()` produces a signature, `apply_headers()` adds auth headers to reqwest requests.

### pmhft-matching

Location: `crates/pmhft-matching/src/`

- `registry.rs` -- `PairRegistry` wraps a `DashMap<String, MatchedPair>` with reverse indexes for O(1) lookup by Polymarket token ID or Kalshi ticker. Thread-safe for concurrent reads/writes.
- `dome_matcher.rs` -- `DomeSportsMatcher` calls Dome's `/matching-markets/sports/{sport}` endpoint and converts results to `MatchedPair` structs. Sports covered: NFL, NBA, MLB, NHL, MLS, NCAAF, NCAAB.
- `fuzzy_matcher.rs` -- `FuzzyMatcher` implements cross-platform pair matching for non-sports markets:
  - `normalize_title()` -- lowercase, strip punctuation, remove stopwords (will, the, be, above, below, etc.)
  - `match_score()` -- 60% Jaro-Winkler similarity + 40% keyword overlap
  - `infer_category()` -- categorizes markets by keyword detection (bitcoin/ethereum → Crypto, trump/election → Politics, etc.)
  - Matches are only attempted within the same category
  - **5 unit tests** for normalization, scoring, and category inference
- `reconciler.rs` -- `PairReconciler` orchestrates periodic refresh: `refresh_sports()` runs per `sports_refresh_interval_sec`, `refresh_fuzzy()` runs per `fuzzy_refresh_interval_sec`. `refresh_once()` runs both immediately (called at startup).

### pmhft-market-data

Location: `crates/pmhft-market-data/src/`

- `feed.rs` -- `MarketDataEvent` enum: `QuoteUpdate`, `BookReset`, `FeedDisconnected`, `FeedReconnected`
- `polymarket_feed.rs` -- `PolymarketFeed` receives `DomeWsMessage` from the WebSocket connection and re-emits as `NormalizedQuote` via broadcast channel
- `kalshi_feed.rs` -- `KalshiFeed` polls Kalshi orderbooks via Dome REST, round-robining through active tickers from the `PairRegistry`. Extracts BBO from bid/ask arrays, computes mid price, emits `NormalizedQuote` for both Yes and No sides.
- `aggregator.rs` -- `MarketDataAggregator` subscribes to the quote broadcast channel, maintains an `L2Book` cache per market, applies snapshots from incoming quotes, and re-emits unified `MarketDataEvent`s

### pmhft-strategy

Location: `crates/pmhft-strategy/src/`

- `fees.rs` -- `FeeModel` with platform-specific fee rates. `net_edge()` computes profit after buying on one platform and selling on the other, accounting for taker fees. `complement_cost()` computes total cost of buying complementary outcomes. **3 unit tests**.
- `spread_tracker.rs` -- `SpreadTracker` maintains a rolling window of `(timestamp_ms, spread_value)` observations. Computes mean, standard deviation, z-score. Estimates Ornstein-Uhlenbeck half-life via AR(1) OLS regression on consecutive spread differences. **2 unit tests**.
- `arb_detector.rs` -- `ArbDetector` implements three detection methods:
  - `check_direct_arb()` -- looks for orderbook crossing (Poly ask < Kalshi bid or vice versa). Two cases per call (buy Poly/sell Kalshi, buy Kalshi/sell Poly).
  - `check_complement_arb()` -- checks if buying Yes on one platform + No on the other costs less than $1.00 after fees.
  - `check_stat_arb()` -- tracks spread z-score, filters by half-life, emits signal when z-score exceeds entry threshold.
  - All methods emit `ArbSignal` via `mpsc::Sender` with signal type, edge, confidence, leg instructions, expected PnL, and TTL.

### pmhft-execution

Location: `crates/pmhft-execution/src/`

- `engine.rs` -- `ExecutionEngine` is the main execution loop. Receives `ArbSignal` from mpsc channel, checks signal freshness (TTL), runs pre-trade risk check, executes both legs via `tokio::join!`, handles all four outcome combinations (both filled, one failed + unwind, both failed). Records telemetry counters.
- `polymarket_exec.rs` -- `PolymarketExecutor` builds a `DomePlaceOrderRequest` (JSON-RPC 2.0) and submits via Dome. Paper trading mode returns simulated fills at limit price.
- `kalshi_exec.rs` -- `KalshiExecutor` prefers FIX (`send_new_order` + wait for `ExecutionReport`) when a `FixSession` is available, falls back to REST (`create_order`). Paper trading mode returns simulated fills. Fill timeout is configurable.

### pmhft-risk

Location: `crates/pmhft-risk/src/`

- `position_tracker.rs` -- `PositionTracker` uses `DashMap` for per-market positions and `AtomicI64` for aggregate metrics (gross exposure in microdollars, realized PnL). `update_from_fill()` adjusts positions and exposure atomically.
- `circuit_breaker.rs` -- `CircuitBreaker` wraps an `AtomicBool`. When tripped, stores the reason. `is_active()` is checked before every trade.
- `risk_manager.rs` -- `RiskManager` combines position tracker, circuit breaker, and config limits. `pre_trade_check()` validates 5 conditions and returns `Err(PmhftError::RiskLimitBreached)` on failure. `post_trade_update()` records fills and checks daily loss limit, tripping the circuit breaker if exceeded.

---

## Testing

### Unit Tests (18 total)

| Crate | Tests | What's Covered |
|---|---|---|
| `pmhft-orderbook` | 7 | Empty book, snapshot apply, delta updates, stale delta rejection, zero-size filtering, bid/ask liquidity aggregation |
| `pmhft-matching` | 5 | Title normalization, identical/similar/different match scores, category inference |
| `pmhft-strategy` | 5 | Net edge (positive/negative), complement cost, z-score calculation, insufficient data handling |
| `pmhft-kalshi-client` | 1 | FIX message encode/decode roundtrip |

### Running Tests

```bash
# All tests
cargo test --workspace

# With output
cargo test --workspace -- --nocapture

# Single test
cargo test -p pmhft-orderbook test_apply_snapshot
```

### Integration Tests (not yet implemented)

These would require live API keys and should be gated behind `#[ignore]`:

```bash
# Run ignored integration tests
cargo test --workspace -- --ignored
```

Potential integration tests:
- Dome REST: fetch markets, prices, orderbooks
- Dome WebSocket: connect, subscribe, receive events
- Kalshi demo API: authenticate, create/cancel order
- Full pipeline: market data → signal → paper execution

---

## Adding a New Crate

1. Create directory under `crates/`:
   ```bash
   mkdir -p crates/pmhft-new-crate/src
   ```

2. Add `Cargo.toml`:
   ```toml
   [package]
   name = "pmhft-new-crate"
   version = "0.1.0"
   edition = "2024"

   [dependencies]
   pmhft-common = { path = "../pmhft-common" }
   # Add workspace deps as needed:
   tokio = { workspace = true }
   ```

3. Add `src/lib.rs` with your module declarations.

4. Register in workspace root `Cargo.toml`:
   ```toml
   [workspace]
   members = [
       # ... existing members
       "crates/pmhft-new-crate",
   ]
   ```

5. Add as dependency to crates that need it.

## Key External Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `tokio` | 1 | Async runtime (full features) |
| `reqwest` | 0.12 | HTTP client (JSON, gzip) |
| `tokio-tungstenite` | 0.24 | WebSocket client |
| `serde` / `serde_json` | 1 | Serialization |
| `rust_decimal` | 1.40 | Exact decimal arithmetic |
| `chrono` | 0.4 | Timestamps |
| `alloy-sol-types` | 0.8 | EIP-712 typed data |
| `alloy-signer-local` | 0.8 | ECDSA wallet signing |
| `rsa` | 0.9 | RSA-PSS for Kalshi auth |
| `hmac` / `sha2` | 0.12 / 0.10 | HMAC-SHA256 for Polymarket L2 |
| `strsim` | 0.11 | Jaro-Winkler fuzzy matching |
| `dashmap` | 6 | Lock-free concurrent maps |
| `metrics` | 0.24 | Prometheus metrics |
| `tracing` | 0.1 | Structured logging |
| `config` | 0.15 | TOML + env config loading |
| `clap` | 4 | CLI argument parsing |
| `tokio-rustls` | 0.26 | TLS for FIX TCP connections |
| `uuid` | 1 | Order and signal IDs |

## Release Build

The release profile is configured for maximum performance:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = "symbols"
```

This produces a single optimized binary with link-time optimization and stripped debug symbols.
