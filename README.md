# Prediction Market HFT

Cross-platform statistical arbitrage system exploiting price discrepancies between **Polymarket** and **Kalshi** prediction markets. Built in Rust for low-latency execution.

## How It Works

The system continuously monitors prices on both Polymarket and Kalshi for equivalent markets, detects mispricings, and executes simultaneous trades on both platforms to capture risk-free (or statistically favorable) profit.

```
Polymarket (WebSocket)  ──> Quote Normalization ──┐
                                                   ├──> Arb Detection ──> Risk Check ──> Execute Both Legs
Kalshi (REST Polling)   ──> Quote Normalization ──┘
```

### Arbitrage Methods

1. **Direct Price Arb** -- Orderbooks cross across platforms (e.g., Poly ask < Kalshi bid). Guaranteed profit if both legs fill.

2. **Complement Arb** -- Buy Yes on one platform + No on the other for total cost < $1.00. Locked profit at market resolution regardless of outcome.

3. **Statistical Arb** -- Track the cross-platform price spread over time. When the spread deviates significantly from its mean (measured by z-score), trade the reversion. Uses Ornstein-Uhlenbeck half-life filtering to reject non-mean-reverting pairs.

### Market Matching

Markets are matched across platforms using two methods:

- **Sports**: Dome API provides pre-matched cross-platform equivalents for NFL, NBA, MLB, NHL, MLS, NCAAF, NCAAB.
- **All other categories** (politics, crypto, weather, economics, entertainment): Custom fuzzy matching using Jaro-Winkler string similarity with keyword overlap scoring.

## Architecture

```
crates/
  pmhft-common/             Shared types, errors, config structs
  pmhft-orderbook/           L2 orderbook (BTreeMap-based, snapshot + delta)
  pmhft-dome-client/         Dome REST (rate-limited) + WebSocket client
  pmhft-kalshi-client/       Kalshi REST (RSA-PSS auth) + FIX 4.4 protocol
  pmhft-polymarket-signer/   EIP-712 order signing + HMAC credentials
  pmhft-matching/            Cross-platform pair discovery (Dome + fuzzy)
  pmhft-market-data/         Feed aggregation, normalized quotes per pair
  pmhft-strategy/            Arb detection engine (direct, complement, stat)
  pmhft-execution/           Dual-leg concurrent order routing
  pmhft-risk/                Position tracking, limits, circuit breaker
  pmhft-telemetry/           Prometheus metrics + structured logging
src/
  main.rs                    CLI entrypoint (clap)
  app.rs                     Application orchestrator
config/
  default.toml               Runtime configuration
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for detailed system design.

## Prerequisites

- **Rust** 1.75+ (2024 edition)
- **Dome API key** -- sign up at [domeapi.io](https://domeapi.io). Free tier (1 req/s) works for development.
- **Kalshi account** -- for REST API access. Demo API (`demo-api.kalshi.co`) used by default. FIX 4.4 access requires separate approval from Kalshi.
- **Polymarket wallet** -- Ethereum private key on Polygon for EIP-712 order signing. L2 CLOB API credentials for authentication.

## Quick Start

```bash
# Clone and build
git clone <repo-url>
cd Prediction-Market-HFT
cargo build --release

# Set API keys via environment variables
export PMHFT__DOME__API_KEY="your-dome-api-key"
export PMHFT__KALSHI__API_KEY_ID="your-kalshi-key-id"
export PMHFT__KALSHI__PRIVATE_KEY_PATH="/path/to/kalshi-rsa-key.pem"
export PMHFT__POLYMARKET__WALLET_PRIVATE_KEY="0x..."

# Run in paper trading mode (default)
cargo run --release

# Or with custom config
cargo run --release -- --config config/default.toml --log-level debug

# Enable live trading (real money)
cargo run --release -- --live
```

## Configuration

All settings are in `config/default.toml` and can be overridden with environment variables using the `PMHFT__` prefix (double underscore separates sections):

```bash
# Examples
PMHFT__DOME__API_KEY="abc123"
PMHFT__STRATEGY__MIN_EDGE_BPS="75"
PMHFT__RISK__MAX_DAILY_LOSS_USD="-300"
PMHFT__EXECUTION__LIVE_TRADING="true"
```

Key settings:

| Setting | Default | Description |
|---|---|---|
| `execution.live_trading` | `false` | Paper trading by default |
| `strategy.min_edge_bps` | `50` | Minimum edge to trade (basis points) |
| `strategy.z_score_entry` | `2.0` | Stat arb entry threshold |
| `risk.max_daily_loss_usd` | `-500` | Circuit breaker trigger |
| `risk.max_gross_exposure_usd` | `10000` | Max total exposure |
| `dome.rate_limit_per_sec` | `1` | Dome free tier limit |

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for the full reference.

## Testing

```bash
# Run all unit tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p pmhft-orderbook
cargo test -p pmhft-strategy
cargo test -p pmhft-matching
```

The test suite covers: orderbook operations (snapshot, delta, BBO, liquidity), fuzzy matching (normalization, scoring, category inference), fee calculations, spread tracking and z-score computation, and FIX 4.4 message encoding/decoding.

## Monitoring

Prometheus metrics are exported on port 9090 (configurable). Key metrics:

- `pmhft.execution.successful_arbs` -- completed arbitrage trades
- `pmhft.execution.signals_expired` -- signals that arrived too late
- `pmhft.execution.signals_risk_rejected` -- signals blocked by risk checks
- `pmhft.execution.partial_fills` -- one leg filled, other failed (triggers unwind)
- `pmhft.execution.both_legs_failed` -- neither leg executed

Structured logging via `tracing` with optional JSON output (`telemetry.enable_json_logs = true`).

## Development

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the crate-by-crate development guide.

```bash
# Check compilation (fast, no codegen)
cargo check --workspace

# Build in release mode (LTO + stripped)
cargo build --release

# Run with debug logging
cargo run -- --log-level debug
```

## Risks and Caveats

- **Leg risk**: The biggest financial danger. If one leg fills and the other doesn't, you have an unhedged directional position. The system uses FOK orders and immediate unwind logic, but latency asymmetry between platforms means this risk cannot be fully eliminated.
- **Fuzzy matching errors**: Auto-matched non-sports pairs could be incorrect. A mismatched pair means you're trading two unrelated markets as if they're equivalent -- guaranteed loss. Low-confidence matches are logged for manual review.
- **Rate limiting**: The Dome free tier (1 req/s) limits how frequently Kalshi orderbooks can be polled. Stale data means stale signals. Upgrade to a paid tier for production use.
- **Market microstructure**: Prediction markets are thin and illiquid compared to traditional financial markets. Slippage can eliminate edge quickly.
- **Regulatory**: Trading on prediction markets may be subject to regulations in your jurisdiction. This software is provided for educational and research purposes.

## License

This project is proprietary. All rights reserved.
