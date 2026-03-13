use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};

use crate::app::bootstrap::BinanceMode;
use crate::dataset::query::{
    backtest_summary_for_path, load_book_ticker_rows_for_path, load_liquidation_events_for_path,
};
use crate::dataset::types::{BacktestDatasetSummary, BookTickerRow, LiquidationEventRow};
use crate::error::storage_error::StorageError;
use crate::strategy::model::StrategyTemplate;

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestConfig {
    pub starting_equity: f64,
    pub risk_pct: f64,
    pub win_rate_assumption: f64,
    pub r_multiple: f64,
    pub max_entry_slippage_pct: f64,
    pub stop_distance_pct: f64,
    pub min_cluster_notional: f64,
    pub cluster_lookback_secs: i64,
    pub failed_hold_timeout_secs: i64,
    pub breakdown_confirm_bps: f64,
    pub cooldown_secs: i64,
    pub taker_fee_rate: f64,
    pub stop_slippage_pct: f64,
    pub tp_slippage_pct: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            starting_equity: 10_000.0,
            risk_pct: 0.005,
            win_rate_assumption: 0.8,
            r_multiple: 1.5,
            max_entry_slippage_pct: 0.001,
            stop_distance_pct: 0.012,
            min_cluster_notional: 1.0,
            cluster_lookback_secs: 60,
            failed_hold_timeout_secs: 30,
            breakdown_confirm_bps: 5.0,
            cooldown_secs: 30,
            taker_fee_rate: 0.0005,
            stop_slippage_pct: 0.0008,
            tp_slippage_pct: 0.0003,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BacktestExitReason {
    TakeProfit,
    StopLoss,
    OpenAtEnd,
}

impl BacktestExitReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TakeProfit => "take_profit",
            Self::StopLoss => "stop_loss",
            Self::OpenAtEnd => "open_at_end",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestTrade {
    pub trade_id: usize,
    pub trigger_time: DateTime<Utc>,
    pub entry_time: DateTime<Utc>,
    pub entry_price: f64,
    pub stop_price: f64,
    pub take_profit_price: f64,
    pub qty: f64,
    pub exit_time: Option<DateTime<Utc>>,
    pub exit_price: Option<f64>,
    pub exit_reason: Option<BacktestExitReason>,
    pub gross_pnl: Option<f64>,
    pub fees: Option<f64>,
    pub net_pnl: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestReport {
    pub run_id: Option<i64>,
    pub template: StrategyTemplate,
    pub instrument: String,
    pub mode: BinanceMode,
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub db_path: PathBuf,
    pub dataset: BacktestDatasetSummary,
    pub config: BacktestConfig,
    pub trigger_count: usize,
    pub trades: Vec<BacktestTrade>,
    pub wins: usize,
    pub losses: usize,
    pub open_trades: usize,
    pub skipped_triggers: usize,
    pub starting_equity: f64,
    pub ending_equity: f64,
    pub net_pnl: f64,
    pub observed_win_rate: f64,
    pub average_net_pnl: f64,
    pub configured_expected_value: f64,
}

#[derive(Debug, Clone)]
struct PendingCluster {
    formed_at_ms: i64,
    zone_low: f64,
}

#[derive(Debug, Clone)]
struct OpenTrade {
    trade_id: usize,
    trigger_time_ms: i64,
    entry_time_ms: i64,
    entry_price: f64,
    stop_price: f64,
    take_profit_price: f64,
    qty: f64,
    entry_fee: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayEventKind {
    Liquidation(usize),
    BookTicker(usize),
}

pub fn run_backtest_for_path(
    db_path: &Path,
    mode: BinanceMode,
    template: StrategyTemplate,
    instrument: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    config: BacktestConfig,
) -> Result<BacktestReport, StorageError> {
    let dataset = backtest_summary_for_path(db_path, mode, instrument, from, to)?;
    let liquidation_events = load_liquidation_events_for_path(db_path, instrument, from, to)?;
    let book_tickers = load_book_ticker_rows_for_path(db_path, instrument, from, to)?;
    Ok(run_backtest_on_events(
        template,
        instrument.to_string(),
        mode,
        from,
        to,
        db_path.to_path_buf(),
        dataset,
        liquidation_events,
        book_tickers,
        config,
    ))
}

fn run_backtest_on_events(
    template: StrategyTemplate,
    instrument: String,
    mode: BinanceMode,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    db_path: PathBuf,
    dataset: BacktestDatasetSummary,
    liquidation_events: Vec<LiquidationEventRow>,
    book_tickers: Vec<BookTickerRow>,
    config: BacktestConfig,
) -> BacktestReport {
    let mut replay = liquidation_events
        .iter()
        .enumerate()
        .map(|(index, event)| (event.event_time_ms, ReplayEventKind::Liquidation(index)))
        .chain(
            book_tickers
                .iter()
                .enumerate()
                .map(|(index, tick)| (tick.event_time_ms, ReplayEventKind::BookTicker(index))),
        )
        .collect::<Vec<_>>();
    replay.sort_by_key(|(time_ms, kind)| {
        (
            *time_ms,
            match kind {
                ReplayEventKind::Liquidation(_) => 0u8,
                ReplayEventKind::BookTicker(_) => 1u8,
            },
        )
    });

    let mut liquidation_window = VecDeque::<LiquidationEventRow>::new();
    let mut pending_cluster: Option<PendingCluster> = None;
    let mut open_trade: Option<OpenTrade> = None;
    let mut completed_trades = Vec::new();
    let mut trigger_count = 0usize;
    let mut skipped_triggers = 0usize;
    let mut next_allowed_entry_ms = 0i64;
    let mut equity = config.starting_equity;

    for (event_time_ms, kind) in replay {
        match kind {
            ReplayEventKind::Liquidation(index) => {
                let event = &liquidation_events[index];
                if event.force_side != "BUY" {
                    continue;
                }
                liquidation_window.push_back(event.clone());
                while liquidation_window.front().is_some_and(|front| {
                    front.event_time_ms < event_time_ms - config.cluster_lookback_secs * 1_000
                }) {
                    let _ = liquidation_window.pop_front();
                }

                let total_notional = liquidation_window
                    .iter()
                    .map(|item| item.notional)
                    .sum::<f64>();
                if open_trade.is_none()
                    && event_time_ms >= next_allowed_entry_ms
                    && total_notional >= config.min_cluster_notional
                {
                    let zone_low = liquidation_window
                        .iter()
                        .map(|item| item.price)
                        .fold(f64::INFINITY, f64::min);
                    pending_cluster = Some(PendingCluster {
                        formed_at_ms: event_time_ms,
                        zone_low,
                    });
                }
            }
            ReplayEventKind::BookTicker(index) => {
                let tick = &book_tickers[index];
                if let Some(trade) = open_trade.as_ref() {
                    if tick.ask >= trade.stop_price {
                        let exit_price = trade.stop_price * (1.0 + config.stop_slippage_pct);
                        let gross_pnl = (trade.entry_price - exit_price) * trade.qty;
                        let exit_fee = exit_price * trade.qty * config.taker_fee_rate;
                        let fees = trade.entry_fee + exit_fee;
                        let net_pnl = gross_pnl - fees;
                        equity += net_pnl;
                        completed_trades.push(BacktestTrade {
                            trade_id: trade.trade_id,
                            trigger_time: timestamp_utc(trade.trigger_time_ms),
                            entry_time: timestamp_utc(trade.entry_time_ms),
                            entry_price: trade.entry_price,
                            stop_price: trade.stop_price,
                            take_profit_price: trade.take_profit_price,
                            qty: trade.qty,
                            exit_time: Some(timestamp_utc(tick.event_time_ms)),
                            exit_price: Some(exit_price),
                            exit_reason: Some(BacktestExitReason::StopLoss),
                            gross_pnl: Some(gross_pnl),
                            fees: Some(fees),
                            net_pnl: Some(net_pnl),
                        });
                        open_trade = None;
                        next_allowed_entry_ms = tick.event_time_ms + config.cooldown_secs * 1_000;
                        continue;
                    }
                    if tick.ask <= trade.take_profit_price {
                        let exit_price = trade.take_profit_price * (1.0 + config.tp_slippage_pct);
                        let gross_pnl = (trade.entry_price - exit_price) * trade.qty;
                        let exit_fee = exit_price * trade.qty * config.taker_fee_rate;
                        let fees = trade.entry_fee + exit_fee;
                        let net_pnl = gross_pnl - fees;
                        equity += net_pnl;
                        completed_trades.push(BacktestTrade {
                            trade_id: trade.trade_id,
                            trigger_time: timestamp_utc(trade.trigger_time_ms),
                            entry_time: timestamp_utc(trade.entry_time_ms),
                            entry_price: trade.entry_price,
                            stop_price: trade.stop_price,
                            take_profit_price: trade.take_profit_price,
                            qty: trade.qty,
                            exit_time: Some(timestamp_utc(tick.event_time_ms)),
                            exit_price: Some(exit_price),
                            exit_reason: Some(BacktestExitReason::TakeProfit),
                            gross_pnl: Some(gross_pnl),
                            fees: Some(fees),
                            net_pnl: Some(net_pnl),
                        });
                        open_trade = None;
                        next_allowed_entry_ms = tick.event_time_ms + config.cooldown_secs * 1_000;
                        continue;
                    }
                }

                let Some(cluster) = pending_cluster.clone() else {
                    continue;
                };
                if open_trade.is_some() || tick.event_time_ms < next_allowed_entry_ms {
                    continue;
                }
                if tick.event_time_ms
                    > cluster.formed_at_ms + config.failed_hold_timeout_secs * 1_000
                {
                    pending_cluster = None;
                    continue;
                }
                let breakdown_price =
                    cluster.zone_low * (1.0 - config.breakdown_confirm_bps / 10_000.0);
                if tick.bid > breakdown_price {
                    continue;
                }
                if equity <= 0.0 {
                    skipped_triggers += 1;
                    pending_cluster = None;
                    continue;
                }

                trigger_count += 1;
                let entry_price = tick.bid * (1.0 - config.max_entry_slippage_pct * 0.5);
                let risk_amount = equity * config.risk_pct;
                let qty = risk_amount / (entry_price * config.stop_distance_pct);
                if !(qty.is_finite() && qty > 0.0) {
                    skipped_triggers += 1;
                    pending_cluster = None;
                    continue;
                }
                let entry_fee = entry_price * qty * config.taker_fee_rate;
                let stop_price = entry_price * (1.0 + config.stop_distance_pct);
                let take_profit_price =
                    entry_price * (1.0 - config.stop_distance_pct * config.r_multiple);
                open_trade = Some(OpenTrade {
                    trade_id: completed_trades.len() + usize::from(open_trade.is_some()) + 1,
                    trigger_time_ms: cluster.formed_at_ms,
                    entry_time_ms: tick.event_time_ms,
                    entry_price,
                    stop_price,
                    take_profit_price,
                    qty,
                    entry_fee,
                });
                pending_cluster = None;
            }
        }
    }

    let mut trades = completed_trades.clone();
    if let Some(trade) = open_trade {
        let last_tick = book_tickers
            .iter()
            .rev()
            .find(|tick| tick.event_time_ms >= trade.entry_time_ms);
        let (exit_time, exit_price, gross_pnl, fees, net_pnl) = if let Some(tick) = last_tick {
            let exit_price = tick.ask;
            let gross_pnl = (trade.entry_price - exit_price) * trade.qty;
            let exit_fee = exit_price * trade.qty * config.taker_fee_rate;
            let fees = trade.entry_fee + exit_fee;
            let net_pnl = gross_pnl - fees;
            equity += net_pnl;
            (
                Some(timestamp_utc(tick.event_time_ms)),
                Some(exit_price),
                Some(gross_pnl),
                Some(fees),
                Some(net_pnl),
            )
        } else {
            (None, None, None, Some(trade.entry_fee), None)
        };
        trades.push(BacktestTrade {
            trade_id: trade.trade_id,
            trigger_time: timestamp_utc(trade.trigger_time_ms),
            entry_time: timestamp_utc(trade.entry_time_ms),
            entry_price: trade.entry_price,
            stop_price: trade.stop_price,
            take_profit_price: trade.take_profit_price,
            qty: trade.qty,
            exit_time,
            exit_price,
            exit_reason: Some(BacktestExitReason::OpenAtEnd),
            gross_pnl,
            fees,
            net_pnl,
        });
    }

    let wins = completed_trades
        .iter()
        .filter(|trade| trade.exit_reason == Some(BacktestExitReason::TakeProfit))
        .count();
    let losses = completed_trades
        .iter()
        .filter(|trade| trade.exit_reason == Some(BacktestExitReason::StopLoss))
        .count();
    let net_pnl = trades.iter().filter_map(|trade| trade.net_pnl).sum::<f64>();
    let realized_trade_count = trades
        .iter()
        .filter(|trade| trade.net_pnl.is_some())
        .count();
    let average_net_pnl = if realized_trade_count == 0 {
        0.0
    } else {
        net_pnl / realized_trade_count as f64
    };
    let observed_win_rate = if completed_trades.is_empty() {
        0.0
    } else {
        wins as f64 / completed_trades.len() as f64
    };
    let average_win = average_net_of(&completed_trades, BacktestExitReason::TakeProfit);
    let average_loss = average_net_of(&completed_trades, BacktestExitReason::StopLoss).abs();
    let configured_expected_value = config.win_rate_assumption * average_win
        - (1.0 - config.win_rate_assumption) * average_loss;

    BacktestReport {
        run_id: None,
        template,
        instrument,
        mode,
        from,
        to,
        db_path,
        dataset,
        config: config.clone(),
        trigger_count,
        trades,
        wins,
        losses,
        open_trades: 0,
        skipped_triggers,
        starting_equity: config.starting_equity,
        ending_equity: equity,
        net_pnl,
        observed_win_rate,
        average_net_pnl,
        configured_expected_value,
    }
}

fn average_net_of(trades: &[BacktestTrade], reason: BacktestExitReason) -> f64 {
    let values = trades
        .iter()
        .filter(|trade| trade.exit_reason == Some(reason.clone()))
        .filter_map(|trade| trade.net_pnl)
        .collect::<Vec<_>>();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn timestamp_utc(event_time_ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(event_time_ms)
        .single()
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liquidation_breakdown_backtest_records_take_profit_trade() {
        let report = run_backtest_on_events(
            StrategyTemplate::LiquidationBreakdownShort,
            "BTCUSDT".to_string(),
            BinanceMode::Demo,
            chrono::NaiveDate::from_ymd_opt(2026, 3, 13).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 3, 13).unwrap(),
            PathBuf::from("/tmp/test.duckdb"),
            BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "BTCUSDT".to_string(),
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                liquidation_events: 1,
                book_ticker_events: 3,
                agg_trade_events: 0,
                derived_kline_1s_bars: 0,
            },
            vec![LiquidationEventRow {
                event_time_ms: 1_000,
                force_side: "BUY".to_string(),
                price: 100.0,
                qty: 100.0,
                notional: 10_000.0,
            }],
            vec![
                BookTickerRow {
                    event_time_ms: 2_000,
                    bid: 99.9,
                    ask: 100.0,
                },
                BookTickerRow {
                    event_time_ms: 3_000,
                    bid: 98.0,
                    ask: 98.0,
                },
            ],
            BacktestConfig::default(),
        );

        assert_eq!(report.wins, 1);
        assert_eq!(report.losses, 0);
        assert!(report.net_pnl > 0.0);
    }

    #[test]
    fn liquidation_breakdown_backtest_records_stop_loss_trade() {
        let report = run_backtest_on_events(
            StrategyTemplate::LiquidationBreakdownShort,
            "BTCUSDT".to_string(),
            BinanceMode::Demo,
            chrono::NaiveDate::from_ymd_opt(2026, 3, 13).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 3, 13).unwrap(),
            PathBuf::from("/tmp/test.duckdb"),
            BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "BTCUSDT".to_string(),
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                liquidation_events: 1,
                book_ticker_events: 3,
                agg_trade_events: 0,
                derived_kline_1s_bars: 0,
            },
            vec![LiquidationEventRow {
                event_time_ms: 1_000,
                force_side: "BUY".to_string(),
                price: 100.0,
                qty: 100.0,
                notional: 10_000.0,
            }],
            vec![
                BookTickerRow {
                    event_time_ms: 2_000,
                    bid: 99.9,
                    ask: 100.0,
                },
                BookTickerRow {
                    event_time_ms: 3_000,
                    bid: 101.2,
                    ask: 101.2,
                },
            ],
            BacktestConfig::default(),
        );

        assert_eq!(report.wins, 0);
        assert_eq!(report.losses, 1);
        assert!(report.net_pnl < 0.0);
    }
}
