use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};

use crate::event::{MarketRegime, MarketRegimeSignal};
use crate::model::candle::Candle;
use crate::model::signal::Signal;
use crate::predictor::{
    build_predictor_models, default_predictor_specs, PredictorBaseConfig, PredictorModel,
};
use crate::runtime::alpha_portfolio::{decide_portfolio_action_from_alpha, RegimeDecisionConfig};
use crate::runtime::predictor_eval::{
    observe_predictor_eval_volatility, predictor_eval_scale, PredictorEvalVolState,
};
use crate::runtime::regime::{RegimeDetector, RegimeDetectorConfig};

const DEFAULT_BACKTEST_TRADE_NOTIONAL_USDT: f64 = 100.0;
const DEFAULT_BACKTEST_FEE_RATE: f64 = 0.001;
const DEFAULT_BACKTEST_SLIPPAGE_BPS: f64 = 2.0;
const DEFAULT_BACKTEST_MIN_SIGNAL_STRENGTH: f64 = 0.0;
const DEFAULT_TRAIN_WINDOW: usize = 420;
const DEFAULT_TEST_WINDOW: usize = 120;
const DEFAULT_EMBARGO_WINDOW: usize = 8;
const DEFAULT_MAX_FOLDS: usize = 5;
const PORTFOLIO_REBALANCE_MIN_DELTA: f64 = 0.05;
const PORTFOLIO_MIN_ENTRY_RATIO: f64 = 0.15;

#[derive(Debug, Clone)]
pub struct BacktestConfig {
    pub symbol: String,
    pub bars_csv: PathBuf,
    pub strategy_db_path: PathBuf,
    pub order_db_path: PathBuf,
    pub order_amount_usdt: f64,
    pub fee_rate: f64,
    pub slippage_bps: f64,
    pub train_window: usize,
    pub test_window: usize,
    pub embargo_window: usize,
    pub max_folds: usize,
    pub min_signal_abs: f64,
    pub regime_gate_enabled: bool,
    pub predictor_ewma_alpha_mean: f64,
    pub predictor_ewma_alpha_var: f64,
    pub predictor_min_sigma: f64,
    pub predictor_mu: f64,
    pub predictor_sigma: f64,
    pub run_seed: u64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            bars_csv: PathBuf::from("data/demo_market_backlog.csv"),
            strategy_db_path: PathBuf::from("data/backtest_strategy.sqlite"),
            order_db_path: PathBuf::from("data/backtest_orders.sqlite"),
            order_amount_usdt: DEFAULT_BACKTEST_TRADE_NOTIONAL_USDT,
            fee_rate: DEFAULT_BACKTEST_FEE_RATE,
            slippage_bps: DEFAULT_BACKTEST_SLIPPAGE_BPS,
            train_window: DEFAULT_TRAIN_WINDOW,
            test_window: DEFAULT_TEST_WINDOW,
            embargo_window: DEFAULT_EMBARGO_WINDOW,
            max_folds: DEFAULT_MAX_FOLDS,
            min_signal_abs: DEFAULT_BACKTEST_MIN_SIGNAL_STRENGTH,
            regime_gate_enabled: false,
            predictor_ewma_alpha_mean: 0.18,
            predictor_ewma_alpha_var: 0.18,
            predictor_min_sigma: 0.001,
            predictor_mu: 0.0,
            predictor_sigma: 0.01,
            run_seed: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WalkWindow {
    pub fold: usize,
    pub train_start: usize,
    pub train_end: usize,
    pub test_start: usize,
    pub test_end: usize,
}

#[derive(Debug, Clone)]
pub struct BacktestMetrics {
    pub realized_pnl_usdt: f64,
    pub total_fees_usdt: f64,
    pub trade_count: u64,
    pub win_count: u64,
    pub lose_count: u64,
    pub max_drawdown: f64,
    pub sharpe_like: f64,
    pub end_equity_usdt: f64,
}

#[derive(Debug, Clone)]
pub struct BacktestFoldResult {
    pub fold: usize,
    pub train_bars: usize,
    pub test_bars: usize,
    pub metrics: BacktestMetrics,
    pub train_start_timestamp_ms: u64,
    pub train_end_timestamp_ms: u64,
    pub start_timestamp_ms: u64,
    pub end_timestamp_ms: u64,
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub run_id: String,
    pub symbol: String,
    pub total_bars: usize,
    pub folds: Vec<BacktestFoldResult>,
    pub metrics: BacktestMetrics,
    pub run_started_ms: u64,
    pub run_finished_ms: u64,
}

#[derive(Debug, Clone)]
pub struct BacktestOrderLedgerRow {
    pub run_id: String,
    pub fold: usize,
    pub order_index: u64,
    pub source: String,
    pub bar_idx: usize,
    pub timestamp_ms: u64,
    pub side: String,
    pub target_ratio: f64,
    pub current_ratio: f64,
    pub qty: f64,
    pub price: f64,
    pub fee_usdt: f64,
    pub pnl_realized_usdt: f64,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct CandleFeed {
    pub symbol: String,
    pub interval_ms: u64,
    pub bars: Vec<Candle>,
}

#[derive(Debug)]
struct BacktestPosition {
    qty: f64,
    entry_price: f64,
    cost_quote: f64,
    realized_pnl: f64,
    unrealized_pnl: f64,
}

impl BacktestPosition {
    fn new() -> Self {
        Self {
            qty: 0.0,
            entry_price: 0.0,
            cost_quote: 0.0,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
        }
    }

    fn is_flat(&self) -> bool {
        self.qty <= f64::EPSILON
    }

    fn notional(&self, price: f64) -> f64 {
        self.qty * price
    }

    fn apply_fill(&mut self, side: Signal, qty: f64, price: f64, fee: f64) -> f64 {
        match side {
            Signal::Buy => {
                if qty <= f64::EPSILON {
                    return 0.0;
                }

                self.qty += qty;
                self.cost_quote += qty * price + fee;
                self.entry_price = self.cost_quote / self.qty.max(f64::EPSILON);
                0.0
            }
            Signal::Sell => {
                if self.qty <= f64::EPSILON || qty <= f64::EPSILON {
                    return 0.0;
                }

                let close_qty = qty.min(self.qty);
                let avg_cost = self.cost_quote / self.qty.max(f64::EPSILON);
                let pnl = close_qty * price - close_qty * avg_cost - fee;
                self.realized_pnl += pnl;
                self.qty -= close_qty;
                self.cost_quote -= close_qty * avg_cost;

                if self.qty <= f64::EPSILON {
                    self.qty = 0.0;
                    self.entry_price = 0.0;
                    self.cost_quote = 0.0;
                } else {
                    self.entry_price = self.cost_quote / self.qty.max(f64::EPSILON);
                }
                pnl
            }
            Signal::Hold => 0.0,
        }
    }

    fn update_unrealized(&mut self, price: f64) {
        if self.is_flat() {
            self.unrealized_pnl = 0.0;
        } else {
            self.unrealized_pnl = (price - self.entry_price) * self.qty;
        }
    }

    fn total_equity(&mut self, close: f64) -> f64 {
        self.update_unrealized(close);
        self.realized_pnl + self.unrealized_pnl
    }
}

pub fn infer_interval_ms(bars: &[Candle]) -> u64 {
    if bars.len() < 2 {
        return 60_000;
    }
    let mut diffs: Vec<u64> = bars
        .windows(2)
        .filter_map(|w| w[1].open_time.checked_sub(w[0].open_time))
        .filter(|d| *d > 0)
        .collect();
    if diffs.is_empty() {
        return 60_000;
    }
    diffs.sort_unstable();
    diffs[diffs.len() / 2]
}

pub fn parse_candle_csv(symbol: &str, path: &Path) -> Result<CandleFeed> {
    let file = File::open(path).with_context(|| format!("open candle csv: {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut bars = Vec::new();
    let mut idx_ts = 0usize;
    let mut idx_open = 1usize;
    let mut idx_high = 2usize;
    let mut idx_low = 3usize;
    let mut idx_close = 4usize;
    let mut line_no = 0usize;
    let mut has_header = false;
    let mut first_row_consumed = false;

    let resolve_idx = |header: &[String], names: &[&str]| -> Option<usize> {
        names
            .iter()
            .find_map(|name| header.iter().position(|h| h == name))
    };

    for raw in reader.lines() {
        let line = raw?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line_no += 1;
            continue;
        }

        let fields: Vec<&str> = trimmed.split(',').map(|v| v.trim()).collect();

        if !has_header {
            let looks_like_header =
                fields.first().is_some_and(|f| f.parse::<f64>().is_err());
            if looks_like_header || first_row_consumed {
                let h = fields
                    .iter()
                    .map(|v| v.to_ascii_lowercase())
                    .collect::<Vec<_>>();
                idx_ts = resolve_idx(&h, &["open_time", "timestamp_ms", "time", "timestamp"])
                    .or_else(|| resolve_idx(&h, &["ts"]))
                    .or(Some(0))
                    .unwrap_or(0);
                idx_open = resolve_idx(&h, &["open"]).unwrap_or(1);
                idx_high = resolve_idx(&h, &["high"]).unwrap_or(2);
                idx_low = resolve_idx(&h, &["low"]).unwrap_or(3);
                idx_close = resolve_idx(&h, &["close"]).unwrap_or(4);
                if idx_open >= fields.len()
                    || idx_high >= fields.len()
                    || idx_low >= fields.len()
                    || idx_close >= fields.len()
                    || idx_ts >= fields.len()
                {
                    return Err(anyhow!(
                        "invalid candle csv header on line {}: {:?}; expected columns including open_time/open and high/low/close",
                        line_no + 1,
                        fields
                    ));
                }
                has_header = looks_like_header;
                if looks_like_header {
                    first_row_consumed = true;
                    line_no += 1;
                    continue;
                }
            }
            has_header = true;
            first_row_consumed = true;
        }

        if fields.len() <= idx_close {
            return Err(anyhow!("invalid row #{}: too few fields", line_no + 1));
        }

        let open_time = parse_u64(fields[idx_ts], line_no)?;
        let open = parse_f64(fields[idx_open], "open", line_no)?;
        let high = parse_f64(fields[idx_high], "high", line_no)?;
        let low = parse_f64(fields[idx_low], "low", line_no)?;
        let close = parse_f64(fields[idx_close], "close", line_no)?;
        if open <= 0.0 || high <= 0.0 || low <= 0.0 || close <= 0.0 {
            return Err(anyhow!("invalid row #{}: non-positive price", line_no + 1));
        }

        bars.push(Candle {
            open,
            high,
            low,
            close,
            open_time,
            close_time: 0,
        });
        line_no += 1;
    }

    if bars.len() < 2 {
        return Err(anyhow!("need at least 2 valid rows, got {}", bars.len()));
    }

    let interval_ms = infer_interval_ms(&bars);
    for i in 0..bars.len().saturating_sub(1) {
        bars[i].close_time = bars[i + 1].open_time;
    }
    let last_close_time = bars
        .last()
        .map(|c| c.open_time + interval_ms)
        .unwrap_or_default();
    if let Some(last) = bars.last_mut() {
        last.close_time = last_close_time;
    }

    Ok(CandleFeed {
        symbol: symbol.to_string(),
        interval_ms,
        bars,
    })
}

fn parse_u64(value: &str, line_no: usize) -> Result<u64> {
    value
        .parse::<u64>()
        .or_else(|_| value.parse::<f64>().map(|v| v.max(0.0) as u64))
        .with_context(|| format!("line {}: parse timestamp", line_no + 1))
}

fn parse_f64(value: &str, field: &str, line_no: usize) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("line {}: parse {} {}", line_no + 1, field, value))
}

pub fn build_walk_forward_windows(total_bars: usize, cfg: &BacktestConfig) -> Vec<WalkWindow> {
    if total_bars < cfg.train_window + cfg.test_window + cfg.embargo_window
        || cfg.train_window == 0
        || cfg.test_window == 0
    {
        return Vec::new();
    }

    let mut folds = Vec::new();
    let mut train_start = 0usize;

    for fold_idx in 0..cfg.max_folds {
        let train_end = train_start + cfg.train_window;
        let test_start = train_end + cfg.embargo_window;
        let test_end = test_start + cfg.test_window;

        if test_end > total_bars {
            break;
        }

        folds.push(WalkWindow {
            fold: fold_idx,
            train_start,
            train_end,
            test_start,
            test_end,
        });

        train_start = test_end;
    }

    folds
}

fn default_regime_signal(now_ms: u64) -> MarketRegimeSignal {
    MarketRegimeSignal {
        regime: MarketRegime::TrendUp,
        confidence: 1.0,
        ema_fast: 0.0,
        ema_slow: 0.0,
        vol_ratio: 0.0,
        slope: 0.0,
        updated_at_ms: now_ms,
    }
}

fn build_predictor_specs(cfg: &BacktestConfig) -> Vec<(String, crate::predictor::PredictorConfig)> {
    let base_cfg = PredictorBaseConfig {
        alpha_mean: cfg.predictor_ewma_alpha_mean,
        alpha_var: cfg.predictor_ewma_alpha_var,
        min_sigma: cfg.predictor_min_sigma,
    };
    default_predictor_specs(base_cfg)
}

pub fn run_walk_forward_backtest(
    cfg: &BacktestConfig,
    feed: &CandleFeed,
) -> Result<BacktestResult> {
    let required = cfg.train_window + cfg.test_window + cfg.embargo_window;
    if feed.bars.len() < required {
        return Err(anyhow!(
            "insufficient bars: got {}, need at least {}",
            feed.bars.len(),
            required
        ));
    }

    let run_started_ms = Utc::now().timestamp_millis() as u64;
    let run_id = format!("bt-{}-{}", cfg.symbol, run_started_ms);
    let windows = build_walk_forward_windows(feed.bars.len(), cfg);
    if windows.is_empty() {
        return Err(anyhow!("no walk-forward folds available"));
    }

    ensure_strategy_db(&cfg.strategy_db_path)?;
    ensure_order_db(&cfg.order_db_path)?;

    let mut order_rows: Vec<BacktestOrderLedgerRow> = Vec::new();
    let mut fold_results: Vec<BacktestFoldResult> = Vec::new();
    let mut run_fold_metrics = Vec::new();
    let mut next_order_index = 0u64;

    for window in windows.iter() {
        let mut models: HashMap<String, PredictorModel> =
            build_predictor_models(&build_predictor_specs(cfg));
        let mut vol_state = PredictorEvalVolState::default();
        let mut regime_detector = RegimeDetector::new(RegimeDetectorConfig::default());
        let mut position = BacktestPosition::new();

        let mut fold_fees = 0.0;
        let mut win_count = 0u64;
        let mut lose_count = 0u64;
        let mut trade_count = 0u64;
        let mut fold_orders = Vec::new();
        let mut fold_equity_curve = Vec::new();
        let mut prev_equity = cfg.order_amount_usdt;

        for idx in window.train_start..window.train_end {
            let close = feed.bars[idx].close;
            if close > f64::EPSILON {
                for m in models.values_mut() {
                    m.observe_price(&cfg.symbol, close);
                }
                observe_predictor_eval_volatility(
                    &mut vol_state,
                    close,
                    cfg.predictor_ewma_alpha_var,
                );
            }
        }

        fold_equity_curve.push(prev_equity);
        for idx in window.test_start..window.test_end {
            let bar = &feed.bars[idx];
            let close = bar.close;
            let now_ms = bar.close_time;

            if close <= f64::EPSILON {
                fold_equity_curve.push(prev_equity);
                continue;
            }

            let regime = if cfg.regime_gate_enabled {
                regime_detector.update(close, now_ms)
            } else {
                default_regime_signal(now_ms)
            };

            for m in models.values_mut() {
                m.observe_price(&cfg.symbol, close);
            }
            observe_predictor_eval_volatility(&mut vol_state, close, cfg.predictor_ewma_alpha_var);
            let norm_scale = predictor_eval_scale(&vol_state, cfg.predictor_min_sigma)
                .max(cfg.predictor_min_sigma);

            let mut selected = ("".to_string(), 0.0f64);
            for (name, model) in models.iter() {
                let pred = model.estimate_base(
                    &cfg.symbol,
                    cfg.predictor_mu,
                    cfg.predictor_sigma.max(cfg.predictor_min_sigma),
                );
                if pred.sigma <= 0.0 {
                    continue;
                }
                let normalized_alpha = pred.mu / norm_scale;
                if normalized_alpha.abs() > selected.1.abs() {
                    selected = (name.clone(), normalized_alpha);
                }
            }

            if selected.0.is_empty() || selected.1.abs() < cfg.min_signal_abs {
                fold_equity_curve.push(prev_equity);
                continue;
            }

            let alpha_mu = selected.1;
            let current_ratio = (position.notional(close)
                / cfg.order_amount_usdt.max(f64::EPSILON))
            .clamp(0.0, 1.0);
            let decision = decide_portfolio_action_from_alpha(
                &cfg.symbol,
                now_ms,
                position.is_flat(),
                alpha_mu,
                cfg.order_amount_usdt,
                regime,
                RegimeDecisionConfig {
                    enabled: cfg.regime_gate_enabled,
                    confidence_min: 0.0,
                    entry_multiplier_trend_up: 1.0,
                    entry_multiplier_range: 1.0,
                    entry_multiplier_trend_down: 1.0,
                    entry_multiplier_unknown: 1.0,
                    hold_multiplier_trend_up: 1.0,
                    hold_multiplier_range: 1.0,
                    hold_multiplier_trend_down: 1.0,
                    hold_multiplier_unknown: 1.0,
                },
            );

            if decision.target_position_ratio < PORTFOLIO_MIN_ENTRY_RATIO {
                fold_equity_curve.push(prev_equity);
                continue;
            }

            let intent = decision.to_intent("bt", cfg.order_amount_usdt, current_ratio);
            let signal = intent.effective_signal(PORTFOLIO_REBALANCE_MIN_DELTA);
            if signal == Signal::Hold {
                fold_equity_curve.push(prev_equity);
                continue;
            }

            let target_qty =
                (cfg.order_amount_usdt * decision.target_position_ratio) / close.max(f64::EPSILON);
            let current_qty = position.qty;
            let delta_qty = target_qty - current_qty;
            let fill_qty = delta_qty.abs();
            if fill_qty <= f64::EPSILON {
                fold_equity_curve.push(prev_equity);
                continue;
            }

            let slippage = cfg.slippage_bps / 10_000.0;
            let fill_price = match signal {
                Signal::Buy => close * (1.0 + slippage),
                Signal::Sell => close * (1.0 - slippage),
                Signal::Hold => close,
            };
            let notional = fill_qty * fill_price;
            let fee = notional * cfg.fee_rate;
            let pnl_realized = position.apply_fill(signal, fill_qty, fill_price, fee);

            fold_fees += fee;
            if signal == Signal::Buy {
                trade_count += 1;
            } else if signal == Signal::Sell {
                trade_count += 1;
                if pnl_realized >= 0.0 {
                    win_count += 1;
                } else {
                    lose_count += 1;
                }
            }

            fold_orders.push(BacktestOrderLedgerRow {
                run_id: run_id.clone(),
                fold: window.fold,
                order_index: next_order_index,
                source: "bt".to_string(),
                bar_idx: idx,
                timestamp_ms: now_ms,
                side: match signal {
                    Signal::Buy => "BUY".to_string(),
                    Signal::Sell => "SELL".to_string(),
                    Signal::Hold => "HOLD".to_string(),
                },
                target_ratio: decision.target_position_ratio,
                current_ratio,
                qty: fill_qty,
                price: fill_price,
                fee_usdt: fee,
                pnl_realized_usdt: pnl_realized,
                reason: decision.reason.to_string(),
            });
            next_order_index = next_order_index.saturating_add(1);

            position.update_unrealized(close);
            let equity = cfg.order_amount_usdt + position.total_equity(close);
            fold_equity_curve.push(equity);
            prev_equity = equity;
        }

        let returns: Vec<f64> = fold_equity_curve
            .windows(2)
            .filter_map(|w| {
                if w[0].abs() <= f64::EPSILON {
                    None
                } else {
                    Some((w[1] - w[0]) / w[0])
                }
            })
            .collect();

        let fold_metrics = BacktestMetrics {
            realized_pnl_usdt: position.realized_pnl,
            total_fees_usdt: fold_fees,
            trade_count,
            win_count,
            lose_count,
            max_drawdown: max_drawdown(&fold_equity_curve),
            sharpe_like: sharpe_like(&returns),
            end_equity_usdt: fold_equity_curve
                .last()
                .copied()
                .unwrap_or(cfg.order_amount_usdt),
        };

        fold_results.push(BacktestFoldResult {
            fold: window.fold,
            train_bars: window.train_end - window.train_start,
            test_bars: window.test_end - window.test_start,
            metrics: fold_metrics.clone(),
            train_start_timestamp_ms: feed.bars[window.train_start].open_time,
            train_end_timestamp_ms: feed.bars[window.train_end - 1].open_time,
            start_timestamp_ms: feed.bars[window.test_start].open_time,
            end_timestamp_ms: feed.bars[window.test_end - 1].open_time,
        });
        run_fold_metrics.push(fold_metrics);
        order_rows.extend(fold_orders);
    }

    persist_run_meta(
        &cfg.strategy_db_path,
        &run_id,
        cfg,
        &feed.symbol,
        run_started_ms,
        Utc::now().timestamp_millis() as u64,
        &fold_results,
    )?;
    persist_fold_results(&cfg.strategy_db_path, &run_id, &fold_results)?;
    persist_order_ledger(&cfg.order_db_path, &order_rows)?;

    let total = summarize_metrics(&run_fold_metrics);
    Ok(BacktestResult {
        run_id,
        symbol: feed.symbol.clone(),
        total_bars: feed.bars.len(),
        folds: fold_results,
        metrics: total,
        run_started_ms,
        run_finished_ms: Utc::now().timestamp_millis() as u64,
    })
}

pub fn parse_backtest_args(args: &[String]) -> Result<BacktestConfig> {
    let mut cfg = BacktestConfig::default();
    let mut i = 0usize;
    let mut bars_set = false;

    while i < args.len() {
        match args[i].as_str() {
            "--symbol" if i + 1 < args.len() => {
                cfg.symbol = args[i + 1].to_string();
                i += 2;
            }
            "--bars" if i + 1 < args.len() => {
                cfg.bars_csv = PathBuf::from(&args[i + 1]);
                bars_set = true;
                i += 2;
            }
            "--strategy-db" if i + 1 < args.len() => {
                cfg.strategy_db_path = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--order-db" if i + 1 < args.len() => {
                cfg.order_db_path = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--order-usdt" if i + 1 < args.len() => {
                cfg.order_amount_usdt = args[i + 1].parse()?;
                i += 2;
            }
            "--fee-rate" if i + 1 < args.len() => {
                cfg.fee_rate = args[i + 1].parse()?;
                i += 2;
            }
            "--slippage-bps" if i + 1 < args.len() => {
                cfg.slippage_bps = args[i + 1].parse()?;
                i += 2;
            }
            "--train-bars" if i + 1 < args.len() => {
                cfg.train_window = args[i + 1].parse()?;
                i += 2;
            }
            "--test-bars" if i + 1 < args.len() => {
                cfg.test_window = args[i + 1].parse()?;
                i += 2;
            }
            "--embargo-bars" if i + 1 < args.len() => {
                cfg.embargo_window = args[i + 1].parse()?;
                i += 2;
            }
            "--max-folds" if i + 1 < args.len() => {
                cfg.max_folds = args[i + 1].parse()?;
                i += 2;
            }
            "--min-signal" if i + 1 < args.len() => {
                cfg.min_signal_abs = args[i + 1].parse()?;
                i += 2;
            }
            "--regime-gate" => {
                cfg.regime_gate_enabled = true;
                i += 1;
            }
            "--help" => {
                return Err(anyhow!(print_backtest_usage()));
            }
            _ => {
                return Err(anyhow!("unknown arg '{}'", args[i]));
            }
        }
    }

    if !cfg.bars_csv.exists() {
        if !bars_set {
            return Err(anyhow!("--bars is required"));
        }
        return Err(anyhow!("bars file not found: {}", cfg.bars_csv.display()));
    }

    if cfg.train_window == 0 || cfg.test_window == 0 {
        return Err(anyhow!("train/test windows must be > 0"));
    }
    if cfg.max_folds == 0 {
        return Err(anyhow!("max-folds must be > 0"));
    }

    Ok(cfg)
}

pub fn print_backtest_usage() -> String {
    let help = [
        "USAGE: backtest [--symbol SYMBOL] --bars FILE [--strategy-db PATH] [--order-db PATH]",
        "               [--order-usdt AMOUNT] [--fee-rate RATE] [--slippage-bps BPS]",
        "               [--train-bars N] [--test-bars N] [--embargo-bars N]",
        "               [--max-folds N] [--min-signal N] [--regime-gate]",
    ];
    help.join("\n")
}

fn summarize_metrics(folds: &[BacktestMetrics]) -> BacktestMetrics {
    let mut realized = 0.0;
    let mut fees = 0.0;
    let mut trade_count = 0u64;
    let mut win_count = 0u64;
    let mut lose_count = 0u64;
    let mut max_dd = 0.0;
    let mut sharpe_sum = 0.0;
    let mut sharpe_count = 0u64;
    let mut end_equity = 0.0;

    for fold in folds {
        realized += fold.realized_pnl_usdt;
        fees += fold.total_fees_usdt;
        trade_count += fold.trade_count;
        win_count += fold.win_count;
        lose_count += fold.lose_count;
        if fold.max_drawdown > max_dd {
            max_dd = fold.max_drawdown;
        }
        if fold.sharpe_like.is_finite() {
            sharpe_sum += fold.sharpe_like;
            sharpe_count += 1;
        }
        if folds.last().is_some_and(|last| std::ptr::eq(last, fold)) {
            end_equity = fold.end_equity_usdt;
        }
    }

    BacktestMetrics {
        realized_pnl_usdt: realized,
        total_fees_usdt: fees,
        trade_count,
        win_count,
        lose_count,
        max_drawdown: max_dd,
        sharpe_like: if sharpe_count == 0 {
            0.0
        } else {
            sharpe_sum / sharpe_count as f64
        },
        end_equity_usdt: end_equity,
    }
}

fn max_drawdown(equity: &[f64]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd = 0.0;

    for &value in equity {
        if value > peak {
            peak = value;
        }
        if peak > 0.0 && value < peak {
            let dd = (peak - value) / peak;
            if dd > max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd
}

fn sharpe_like(returns: &[f64]) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let var = returns.iter().map(|r| (*r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    let sd = var.sqrt();
    if sd <= f64::EPSILON {
        0.0
    } else {
        mean / sd
    }
}

fn ensure_strategy_db(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS backtest_runs (
            run_id TEXT PRIMARY KEY,
            symbol TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            finished_at_ms INTEGER NOT NULL,
            config_json TEXT NOT NULL,
            total_bars INTEGER NOT NULL,
            folds INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS backtest_fold_results (
            run_id TEXT NOT NULL,
            fold_idx INTEGER NOT NULL,
            train_bars INTEGER NOT NULL,
            test_bars INTEGER NOT NULL,
            realized_pnl_usdt REAL NOT NULL,
            total_fees_usdt REAL NOT NULL,
            trade_count INTEGER NOT NULL,
            win_count INTEGER NOT NULL,
            lose_count INTEGER NOT NULL,
            max_drawdown REAL NOT NULL,
            sharpe_like REAL NOT NULL,
            end_equity_usdt REAL NOT NULL,
            train_start_ms INTEGER NOT NULL,
            train_end_ms INTEGER NOT NULL,
            test_start_ms INTEGER NOT NULL,
            test_end_ms INTEGER NOT NULL,
            PRIMARY KEY(run_id, fold_idx)
        );
        "#,
    )?;
    Ok(())
}

fn ensure_order_db(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS backtest_orders (
            run_id TEXT NOT NULL,
            fold INTEGER NOT NULL,
            order_index INTEGER NOT NULL,
            source TEXT NOT NULL,
            bar_idx INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            side TEXT NOT NULL,
            target_ratio REAL NOT NULL,
            current_ratio REAL NOT NULL,
            qty REAL NOT NULL,
            price REAL NOT NULL,
            fee_usdt REAL NOT NULL,
            pnl_realized_usdt REAL NOT NULL,
            reason TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(run_id, fold, order_index)
        );
        "#,
    )?;
    Ok(())
}

fn persist_run_meta(
    path: &Path,
    run_id: &str,
    cfg: &BacktestConfig,
    symbol: &str,
    started_ms: u64,
    finished_ms: u64,
    folds: &[BacktestFoldResult],
) -> Result<()> {
    let conn = Connection::open(path)?;
    let cfg_json = serde_json::json!({
        "symbol": cfg.symbol,
        "bars_csv": cfg.bars_csv,
        "order_amount_usdt": cfg.order_amount_usdt,
        "fee_rate": cfg.fee_rate,
        "slippage_bps": cfg.slippage_bps,
        "train_window": cfg.train_window,
        "test_window": cfg.test_window,
        "embargo_window": cfg.embargo_window,
        "max_folds": cfg.max_folds,
        "min_signal_abs": cfg.min_signal_abs,
        "regime_gate_enabled": cfg.regime_gate_enabled,
    });

    conn.execute(
        "INSERT OR REPLACE INTO backtest_runs (
            run_id, symbol, started_at_ms, finished_at_ms, config_json, total_bars, folds, created_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            run_id,
            symbol,
            started_ms as i64,
            finished_ms as i64,
            cfg_json.to_string(),
            folds.iter().map(|f| f.train_bars + f.test_bars).sum::<usize>() as i64,
            folds.len() as i64,
            Utc::now().timestamp_millis() as i64,
        ],
    )?;
    Ok(())
}

fn persist_fold_results(path: &Path, run_id: &str, folds: &[BacktestFoldResult]) -> Result<()> {
    let mut conn = Connection::open(path)?;
    let tx = conn.transaction()?;
    for fold in folds {
        tx.execute(
            "INSERT INTO backtest_fold_results (
                run_id, fold_idx, train_bars, test_bars, realized_pnl_usdt, total_fees_usdt,
                trade_count, win_count, lose_count, max_drawdown, sharpe_like, end_equity_usdt,
                train_start_ms, train_end_ms, test_start_ms, test_end_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                run_id,
                fold.fold as i64,
                fold.train_bars as i64,
                fold.test_bars as i64,
                fold.metrics.realized_pnl_usdt,
                fold.metrics.total_fees_usdt,
                fold.metrics.trade_count as i64,
                fold.metrics.win_count as i64,
                fold.metrics.lose_count as i64,
                fold.metrics.max_drawdown,
                fold.metrics.sharpe_like,
                fold.metrics.end_equity_usdt,
                fold.train_start_timestamp_ms as i64,
                fold.train_end_timestamp_ms as i64,
                fold.start_timestamp_ms as i64,
                fold.end_timestamp_ms as i64,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn persist_order_ledger(path: &Path, rows: &[BacktestOrderLedgerRow]) -> Result<()> {
    let mut conn = Connection::open(path)?;
    let tx = conn.transaction()?;
    for row in rows {
        tx.execute(
            "INSERT OR REPLACE INTO backtest_orders (
                run_id, fold, order_index, source, bar_idx, timestamp_ms, side,
                target_ratio, current_ratio, qty, price, fee_usdt, pnl_realized_usdt, reason, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                row.run_id,
                row.fold as i64,
                row.order_index as i64,
                row.source,
                row.bar_idx as i64,
                row.timestamp_ms as i64,
                row.side,
                row.target_ratio,
                row.current_ratio,
                row.qty,
                row.price,
                row.fee_usdt,
                row.pnl_realized_usdt,
                row.reason,
                Utc::now().timestamp_millis() as i64,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}
