# Configuration Reference

All settings are defined in `config/default.toml`. Every setting can be overridden via environment variables using the `PMHFT__` prefix with double-underscore section separators.

```bash
# Environment variable pattern:
PMHFT__{SECTION}__{KEY}=value

# Examples:
PMHFT__DOME__API_KEY="your-key"
PMHFT__STRATEGY__MIN_EDGE_BPS="75"
PMHFT__EXECUTION__LIVE_TRADING="true"
```

CLI flags take highest precedence:

```bash
pmhft --config config/default.toml --live --log-level debug
```

Precedence order: CLI flags > environment variables > config file > defaults.

---

## `[dome]` -- Dome API

| Key | Type | Default | Description |
|---|---|---|---|
| `api_key` | string | `""` | Dome API key. **Set via `PMHFT__DOME__API_KEY`**. |
| `base_url` | string | `https://api.domeapi.io/v1` | Dome REST API base URL. |
| `ws_url` | string | `wss://ws.domeapi.io` | Dome WebSocket URL. |
| `rate_limit_per_sec` | u32 | `1` | Max requests/second. Free=1, Dev=100, Pro=300. |
| `request_timeout_ms` | u64 | `5000` | HTTP request timeout in milliseconds. |

The rate limiter enforces this limit across all Dome REST endpoints (market data, order placement, pair discovery). WebSocket connections are not rate-limited.

---

## `[kalshi]` -- Kalshi Exchange

| Key | Type | Default | Description |
|---|---|---|---|
| `api_key_id` | string | `""` | Kalshi API key ID. **Set via `PMHFT__KALSHI__API_KEY_ID`**. |
| `private_key_path` | string | `""` | Path to RSA private key PEM file for signing. |
| `rest_base_url` | string | `https://demo-api.kalshi.co/trade-api/v2` | REST API base URL. Use `demo-api` for development, `trading-api.kalshi.com` for production. |
| `fix_host` | string | `""` | FIX 4.4 gateway hostname. Leave empty to disable FIX. |
| `fix_port` | u16 | `0` | FIX gateway port. |
| `fix_sender_comp_id` | string | `""` | FIX SenderCompID (assigned by Kalshi). |
| `fix_target_comp_id` | string | `""` | FIX TargetCompID (assigned by Kalshi). |

FIX 4.4 access requires a separate application/approval process with Kalshi. When FIX is not configured, the system falls back to REST for Kalshi order execution.

---

## `[polymarket]` -- Polymarket

| Key | Type | Default | Description |
|---|---|---|---|
| `wallet_private_key` | string | `""` | Ethereum private key (hex) for EIP-712 signing. **Set via `PMHFT__POLYMARKET__WALLET_PRIVATE_KEY`**. |
| `chain_id` | u64 | `137` | Polygon chain ID. 137=mainnet, 80001=Mumbai testnet. |
| `exchange_address` | string | `0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E` | Polymarket exchange contract address. |
| `api_key` | string | `""` | L2 CLOB API key. |
| `api_secret` | string | `""` | L2 CLOB API secret (for HMAC signing). |
| `api_passphrase` | string | `""` | L2 CLOB API passphrase. |

---

## `[matching]` -- Pair Discovery

| Key | Type | Default | Description |
|---|---|---|---|
| `sports_refresh_interval_sec` | u64 | `900` | How often to refresh sports pairs via Dome (seconds). |
| `fuzzy_refresh_interval_sec` | u64 | `1800` | How often to run fuzzy matching for non-sports markets (seconds). |
| `fuzzy_similarity_threshold` | f64 | `0.85` | Minimum Jaro-Winkler + keyword overlap score to accept a fuzzy match. Range: 0.0-1.0. |
| `min_volume_usd` | f64 | `1000.0` | Minimum daily volume (USD) for a market to be considered for matching. |
| `max_spread_pct` | f64 | `10.0` | Maximum bid-ask spread (%) for a market to be tradeable. |
| `exclude_near_expiry_hours` | u64 | `2` | Exclude markets expiring within this many hours. |

The fuzzy matcher uses a weighted combination: 60% Jaro-Winkler string similarity on normalized titles + 40% keyword overlap scoring. Matches are only attempted within the same inferred category (sports, politics, crypto, weather, etc.).

---

## `[strategy]` -- Arbitrage Detection

| Key | Type | Default | Description |
|---|---|---|---|
| `min_edge_bps` | Decimal | `50` | Minimum net edge (after fees) in basis points to emit a signal. 50 bps = 0.50%. |
| `z_score_entry` | Decimal | `2.0` | Z-score threshold to enter a stat arb trade. |
| `z_score_exit` | Decimal | `0.5` | Z-score threshold to exit a stat arb position. |
| `spread_window` | usize | `100` | Number of spread observations for rolling statistics. |
| `max_half_life_sec` | f64 | `3600.0` | Maximum acceptable Ornstein-Uhlenbeck half-life (seconds). Pairs with longer half-lives are rejected. |
| `max_quote_age_ms` | u64 | `5000` | Maximum quote age before it's considered stale. Stale quotes are rejected by the arb detector. |

**Tuning guidance:**
- Lower `min_edge_bps` = more signals but lower quality. Start at 50 and reduce as you gain confidence.
- Higher `z_score_entry` = fewer but higher-conviction stat arb trades. 2.0 is standard.
- Shorter `spread_window` = faster reaction to regime changes but noisier statistics.
- Shorter `max_half_life_sec` = only trade fast-reverting spreads (safer but fewer opportunities).

---

## `[risk]` -- Risk Management

| Key | Type | Default | Description |
|---|---|---|---|
| `max_position_per_market` | Decimal | `500` | Maximum contracts held in any single market. |
| `max_gross_exposure_usd` | Decimal | `10000` | Maximum total dollar exposure across all positions. |
| `max_order_notional_usd` | Decimal | `1000` | Maximum notional value for a single order. |
| `max_daily_loss_usd` | Decimal | `-500` | Daily loss threshold that triggers the circuit breaker. Must be negative. |
| `max_open_orders` | u32 | `20` | Maximum number of concurrent open orders. |

When `max_daily_loss_usd` is breached, the circuit breaker trips: all new signals are rejected until the system is restarted or the breaker is manually reset.

---

## `[execution]` -- Order Execution

| Key | Type | Default | Description |
|---|---|---|---|
| `live_trading` | bool | `false` | `false` = paper trading (simulated fills), `true` = real orders. |
| `default_order_type` | string | `FOK` | Default order type: `FOK` (Fill or Kill), `FAK` (Fill and Kill), `GTC` (Good Till Cancelled). |
| `use_fix_for_kalshi` | bool | `true` | Use FIX 4.4 for Kalshi execution. `false` = REST fallback. |
| `use_dome_for_polymarket` | bool | `true` | Use Dome's placeOrder for Polymarket execution. |
| `max_slippage_bps` | Decimal | `20` | Maximum acceptable slippage in basis points. |
| `fill_timeout_ms` | u64 | `3000` | Timeout waiting for fill confirmation (milliseconds). |

FOK is the recommended order type for arbitrage: the order either fills completely and immediately, or is cancelled. This minimizes leg risk.

---

## `[telemetry]` -- Observability

| Key | Type | Default | Description |
|---|---|---|---|
| `log_level` | string | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error`. |
| `prometheus_port` | u16 | `9090` | Port for the Prometheus metrics HTTP exporter. |
| `enable_json_logs` | bool | `false` | `true` = structured JSON log output, `false` = human-readable. |

---

## Example: Minimal Paper Trading Config

```bash
# Only Dome key required for paper trading
export PMHFT__DOME__API_KEY="your-dome-api-key"
cargo run --release
```

## Example: Production Config

```bash
export PMHFT__DOME__API_KEY="your-dome-api-key"
export PMHFT__KALSHI__API_KEY_ID="your-kalshi-key"
export PMHFT__KALSHI__PRIVATE_KEY_PATH="/secrets/kalshi-rsa.pem"
export PMHFT__POLYMARKET__WALLET_PRIVATE_KEY="0xabc..."
export PMHFT__POLYMARKET__API_KEY="your-l2-api-key"
export PMHFT__POLYMARKET__API_SECRET="your-l2-api-secret"
export PMHFT__POLYMARKET__API_PASSPHRASE="your-l2-passphrase"
export PMHFT__DOME__RATE_LIMIT_PER_SEC="100"  # Dev tier
export PMHFT__STRATEGY__MIN_EDGE_BPS="30"
export PMHFT__RISK__MAX_DAILY_LOSS_USD="-200"
cargo run --release -- --live
```
