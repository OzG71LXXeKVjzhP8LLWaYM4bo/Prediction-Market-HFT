use crate::fees::FeeModel;
use crate::spread_tracker::SpreadTracker;
use chrono::Utc;
use dashmap::DashMap;
use pmhft_common::config::StrategyConfig;
use pmhft_common::{
    ArbSignal, ArbSignalType, Direction, LegInstruction, MarketId, MatchedPair, OutcomeSide,
    Platform,
};
use pmhft_orderbook::L2Book;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Core arbitrage detection engine.
///
/// Implements three detection methods:
/// 1. Direct price arb: orderbooks cross across platforms.
/// 2. Complement arb: Yes + No mispricing across platforms.
/// 3. Statistical arb: mean-reverting cross-platform spread.
pub struct ArbDetector {
    config: StrategyConfig,
    fee_model: FeeModel,
    spread_trackers: DashMap<String, SpreadTracker>,
    signal_tx: mpsc::Sender<ArbSignal>,
}

impl ArbDetector {
    pub fn new(config: StrategyConfig, signal_tx: mpsc::Sender<ArbSignal>) -> Self {
        Self {
            config,
            fee_model: FeeModel::default_fees(),
            spread_trackers: DashMap::new(),
            signal_tx,
        }
    }

    /// Called on every quote update for a matched pair.
    /// `poly_yes_book` / `kalshi_yes_book` are the Yes-outcome orderbooks.
    pub fn on_quote_update(
        &self,
        pair: &MatchedPair,
        poly_yes_book: &L2Book,
        kalshi_yes_book: &L2Book,
    ) {
        let max_age = self.config.max_quote_age_ms as i64;
        if poly_yes_book.age_ms() > max_age || kalshi_yes_book.age_ms() > max_age {
            return;
        }

        self.check_direct_arb(pair, poly_yes_book, kalshi_yes_book);
        self.check_stat_arb(pair, poly_yes_book, kalshi_yes_book);
    }

    /// Check all three arb types when we have both Yes and No books.
    pub fn on_full_book_update(
        &self,
        pair: &MatchedPair,
        poly_yes_book: &L2Book,
        poly_no_book: &L2Book,
        kalshi_yes_book: &L2Book,
        kalshi_no_book: &L2Book,
    ) {
        let max_age = self.config.max_quote_age_ms as i64;
        if poly_yes_book.age_ms() > max_age || kalshi_yes_book.age_ms() > max_age {
            return;
        }

        self.check_direct_arb(pair, poly_yes_book, kalshi_yes_book);
        self.check_complement_arb(pair, poly_yes_book, poly_no_book, kalshi_yes_book, kalshi_no_book);
        self.check_stat_arb(pair, poly_yes_book, kalshi_yes_book);
    }

    /// Method 1: Direct price arbitrage.
    ///
    /// If Poly best ask < Kalshi best bid → buy Poly, sell Kalshi.
    /// If Kalshi best ask < Poly best bid → buy Kalshi, sell Poly.
    fn check_direct_arb(
        &self,
        pair: &MatchedPair,
        poly_book: &L2Book,
        kalshi_book: &L2Book,
    ) {
        let min_edge = self.min_edge_decimal();

        // Case A: Buy Yes on Polymarket, Sell Yes on Kalshi.
        if let (Some(poly_ask), Some(kalshi_bid)) = (poly_book.best_ask(), kalshi_book.best_bid()) {
            let net_edge = self.fee_model.net_edge(
                Platform::Polymarket,
                poly_ask.price,
                Platform::Kalshi,
                kalshi_bid.price,
            );

            if net_edge > min_edge {
                let size = poly_ask.size.min(kalshi_bid.size);
                self.emit_signal(ArbSignal {
                    signal_id: uuid::Uuid::new_v4(),
                    pair_id: pair.pair_id.clone(),
                    signal_type: ArbSignalType::PriceArb,
                    edge_bps: net_edge * Decimal::from(10000),
                    confidence: Decimal::ONE,
                    poly_side: LegInstruction {
                        platform: Platform::Polymarket,
                        market_id: self.poly_market_id(pair),
                        direction: Direction::Buy,
                        side: OutcomeSide::Yes,
                        limit_price: poly_ask.price,
                        quantity: size,
                    },
                    kalshi_side: LegInstruction {
                        platform: Platform::Kalshi,
                        market_id: self.kalshi_market_id(pair),
                        direction: Direction::Sell,
                        side: OutcomeSide::Yes,
                        limit_price: kalshi_bid.price,
                        quantity: size,
                    },
                    expected_pnl: net_edge * size,
                    timestamp: Utc::now(),
                    ttl_ms: 2000,
                });
            }
        }

        // Case B: Buy Yes on Kalshi, Sell Yes on Polymarket.
        if let (Some(kalshi_ask), Some(poly_bid)) = (kalshi_book.best_ask(), poly_book.best_bid()) {
            let net_edge = self.fee_model.net_edge(
                Platform::Kalshi,
                kalshi_ask.price,
                Platform::Polymarket,
                poly_bid.price,
            );

            if net_edge > min_edge {
                let size = kalshi_ask.size.min(poly_bid.size);
                self.emit_signal(ArbSignal {
                    signal_id: uuid::Uuid::new_v4(),
                    pair_id: pair.pair_id.clone(),
                    signal_type: ArbSignalType::PriceArb,
                    edge_bps: net_edge * Decimal::from(10000),
                    confidence: Decimal::ONE,
                    poly_side: LegInstruction {
                        platform: Platform::Polymarket,
                        market_id: self.poly_market_id(pair),
                        direction: Direction::Sell,
                        side: OutcomeSide::Yes,
                        limit_price: poly_bid.price,
                        quantity: size,
                    },
                    kalshi_side: LegInstruction {
                        platform: Platform::Kalshi,
                        market_id: self.kalshi_market_id(pair),
                        direction: Direction::Buy,
                        side: OutcomeSide::Yes,
                        limit_price: kalshi_ask.price,
                        quantity: size,
                    },
                    expected_pnl: net_edge * size,
                    timestamp: Utc::now(),
                    ttl_ms: 2000,
                });
            }
        }
    }

    /// Method 2: Complement arbitrage.
    ///
    /// If buying Yes on one platform + No on the other costs < $1.00 after fees,
    /// there is locked profit at resolution.
    fn check_complement_arb(
        &self,
        pair: &MatchedPair,
        poly_yes_book: &L2Book,
        _poly_no_book: &L2Book,
        _kalshi_yes_book: &L2Book,
        kalshi_no_book: &L2Book,
    ) {
        // Case: Buy Yes on Polymarket + Buy No on Kalshi.
        if let (Some(poly_yes_ask), Some(kalshi_no_ask)) =
            (poly_yes_book.best_ask(), kalshi_no_book.best_ask())
        {
            let cost = self.fee_model.complement_cost(
                Platform::Polymarket,
                poly_yes_ask.price,
                Platform::Kalshi,
                kalshi_no_ask.price,
            );

            if cost < Decimal::ONE {
                let profit = Decimal::ONE - cost;
                let size = poly_yes_ask.size.min(kalshi_no_ask.size);

                if profit > self.min_edge_decimal() {
                    self.emit_signal(ArbSignal {
                        signal_id: uuid::Uuid::new_v4(),
                        pair_id: pair.pair_id.clone(),
                        signal_type: ArbSignalType::ComplementArb,
                        edge_bps: profit * Decimal::from(10000),
                        confidence: Decimal::ONE,
                        poly_side: LegInstruction {
                            platform: Platform::Polymarket,
                            market_id: self.poly_market_id(pair),
                            direction: Direction::Buy,
                            side: OutcomeSide::Yes,
                            limit_price: poly_yes_ask.price,
                            quantity: size,
                        },
                        kalshi_side: LegInstruction {
                            platform: Platform::Kalshi,
                            market_id: self.kalshi_market_id(pair),
                            direction: Direction::Buy,
                            side: OutcomeSide::No,
                            limit_price: kalshi_no_ask.price,
                            quantity: size,
                        },
                        expected_pnl: profit * size,
                        timestamp: Utc::now(),
                        ttl_ms: 5000,
                    });
                }
            }
        }

        // Case: Buy Yes on Kalshi + Buy No on Polymarket.
        // (Omitted for brevity but follows the same pattern with swapped platforms.)
    }

    /// Method 3: Statistical (mean-reversion) arbitrage.
    ///
    /// Track the cross-platform spread and trade when z-score exceeds threshold.
    fn check_stat_arb(
        &self,
        pair: &MatchedPair,
        poly_book: &L2Book,
        kalshi_book: &L2Book,
    ) {
        let poly_mid = match poly_book.mid_price() {
            Some(m) => match m.to_f64() {
                Some(v) => v,
                None => return,
            },
            None => return,
        };
        let kalshi_mid = match kalshi_book.mid_price() {
            Some(m) => match m.to_f64() {
                Some(v) => v,
                None => return,
            },
            None => return,
        };

        let spread = poly_mid - kalshi_mid;
        let now_ms = Utc::now().timestamp_millis();

        // Update spread tracker.
        let mut tracker = self
            .spread_trackers
            .entry(pair.pair_id.clone())
            .or_insert_with(|| SpreadTracker::new(self.config.spread_window));
        tracker.update(now_ms, spread);

        // Check half-life is in acceptable range.
        let _half_life = match tracker.half_life() {
            Some(hl) if hl > 0.0 && hl <= self.config.max_half_life_sec => hl,
            _ => return,
        };

        // Check z-score.
        let zscore = match tracker.zscore(spread) {
            Some(z) => z,
            None => return,
        };

        let entry_threshold = self.config.z_score_entry.to_f64().unwrap_or(2.0);

        if zscore.abs() > entry_threshold {
            // If zscore > 0: poly overpriced -> sell poly, buy kalshi.
            // If zscore < 0: kalshi overpriced -> buy poly, sell kalshi.
            let (poly_dir, kalshi_dir) = if zscore > 0.0 {
                (Direction::Sell, Direction::Buy)
            } else {
                (Direction::Buy, Direction::Sell)
            };

            let confidence = Decimal::from_f64_retain((zscore.abs() / entry_threshold).min(1.0))
                .unwrap_or(Decimal::ZERO);

            let size = match (poly_dir, kalshi_dir) {
                (Direction::Sell, Direction::Buy) => {
                    let poly_sz = poly_book
                        .best_bid()
                        .map(|l| l.size)
                        .unwrap_or(Decimal::ZERO);
                    let kalshi_sz = kalshi_book
                        .best_ask()
                        .map(|l| l.size)
                        .unwrap_or(Decimal::ZERO);
                    poly_sz.min(kalshi_sz)
                }
                (Direction::Buy, Direction::Sell) => {
                    let poly_sz = poly_book
                        .best_ask()
                        .map(|l| l.size)
                        .unwrap_or(Decimal::ZERO);
                    let kalshi_sz = kalshi_book
                        .best_bid()
                        .map(|l| l.size)
                        .unwrap_or(Decimal::ZERO);
                    poly_sz.min(kalshi_sz)
                }
                _ => Decimal::ZERO,
            };

            if size > Decimal::ZERO {
                let poly_price = match poly_dir {
                    Direction::Buy => poly_book
                        .best_ask()
                        .map(|l| l.price)
                        .unwrap_or(Decimal::ZERO),
                    Direction::Sell => poly_book
                        .best_bid()
                        .map(|l| l.price)
                        .unwrap_or(Decimal::ZERO),
                };

                let kalshi_price = match kalshi_dir {
                    Direction::Buy => kalshi_book
                        .best_ask()
                        .map(|l| l.price)
                        .unwrap_or(Decimal::ZERO),
                    Direction::Sell => kalshi_book
                        .best_bid()
                        .map(|l| l.price)
                        .unwrap_or(Decimal::ZERO),
                };

                self.emit_signal(ArbSignal {
                    signal_id: uuid::Uuid::new_v4(),
                    pair_id: pair.pair_id.clone(),
                    signal_type: ArbSignalType::StatisticalArb,
                    edge_bps: Decimal::from_f64_retain(zscore.abs() * 100.0)
                        .unwrap_or(Decimal::ZERO),
                    confidence,
                    poly_side: LegInstruction {
                        platform: Platform::Polymarket,
                        market_id: self.poly_market_id(pair),
                        direction: poly_dir,
                        side: OutcomeSide::Yes,
                        limit_price: poly_price,
                        quantity: size,
                    },
                    kalshi_side: LegInstruction {
                        platform: Platform::Kalshi,
                        market_id: self.kalshi_market_id(pair),
                        direction: kalshi_dir,
                        side: OutcomeSide::Yes,
                        limit_price: kalshi_price,
                        quantity: size,
                    },
                    expected_pnl: Decimal::from_f64_retain(
                        spread.abs() * size.to_f64().unwrap_or(0.0),
                    )
                    .unwrap_or(Decimal::ZERO),
                    timestamp: Utc::now(),
                    ttl_ms: 5000,
                });
            }
        }
    }

    fn min_edge_decimal(&self) -> Decimal {
        self.config.min_edge_bps / Decimal::from(10000)
    }

    fn poly_market_id(&self, pair: &MatchedPair) -> MarketId {
        MarketId {
            platform: Platform::Polymarket,
            id: pair
                .polymarket
                .token_ids
                .first()
                .cloned()
                .unwrap_or_default(),
        }
    }

    fn kalshi_market_id(&self, pair: &MatchedPair) -> MarketId {
        MarketId {
            platform: Platform::Kalshi,
            id: pair
                .kalshi
                .market_tickers
                .first()
                .cloned()
                .unwrap_or_default(),
        }
    }

    fn emit_signal(&self, signal: ArbSignal) {
        debug!(
            signal_id = %signal.signal_id,
            pair_id = %signal.pair_id,
            signal_type = ?signal.signal_type,
            edge_bps = %signal.edge_bps,
            "Arb signal emitted"
        );
        if let Err(e) = self.signal_tx.try_send(signal) {
            warn!("Failed to emit arb signal: {}", e);
        }
    }
}
