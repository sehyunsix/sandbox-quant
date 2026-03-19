use chrono::Datelike;

use crate::backtest_app::runner::BacktestReport;
use crate::charting::scene::{
    Bar, BarSeries, Candle, CandleSeries, ChartScene, EpochMs, HoverModel, LinePoint, LineSeries,
    Marker, MarkerSeries, MarkerShape, Pane, Series, TooltipModel, ValueFormatter, Viewport,
    YAxisSpec,
};
use crate::charting::style::{ChartTheme, RgbColor};
use crate::visualization::service::VisualizationService;
use crate::visualization::types::{DashboardSnapshot, SignalKind};

const PRICE: RgbColor = RgbColor::new(120, 220, 180);
const LIQ_BUY: RgbColor = RgbColor::new(255, 140, 90);
const LIQ_OTHER: RgbColor = RgbColor::new(255, 210, 100);
const ENTRY: RgbColor = RgbColor::new(90, 170, 255);
const TAKE_PROFIT: RgbColor = RgbColor::new(80, 220, 140);
const STOP_LOSS: RgbColor = RgbColor::new(255, 90, 90);
const OPEN_AT_END: RgbColor = RgbColor::new(240, 220, 120);
const SIGNAL_EXIT: RgbColor = RgbColor::new(200, 200, 255);
const EQUITY: RgbColor = RgbColor::new(120, 180, 255);
const VOLUME_UP: RgbColor = RgbColor::new(70, 150, 110);
const VOLUME_DOWN: RgbColor = RgbColor::new(160, 90, 90);
const SECONDARY_LINE: RgbColor = RgbColor::new(255, 215, 90);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketTimeframe {
    Tick1s,
    Minute1m,
    Minute3m,
    Minute5m,
    Minute15m,
    Minute30m,
    Hour1h,
    Hour4h,
    Week1w,
    Day1d,
    Month1mo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketSeriesKind {
    Candles,
    MidPrice,
    CloseLine,
    Ema20,
    Vwap,
    Liquidations,
}

impl MarketSeriesKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Candles => "Candles",
            Self::MidPrice => "Mid-price",
            Self::CloseLine => "Close line",
            Self::Ema20 => "EMA 20",
            Self::Vwap => "VWAP",
            Self::Liquidations => "Liquidations",
        }
    }

    pub fn all() -> [Self; 6] {
        [
            Self::Candles,
            Self::MidPrice,
            Self::CloseLine,
            Self::Ema20,
            Self::Vwap,
            Self::Liquidations,
        ]
    }
}

impl MarketTimeframe {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tick1s => "1s",
            Self::Minute1m => "1m",
            Self::Minute3m => "3m",
            Self::Minute5m => "5m",
            Self::Minute15m => "15m",
            Self::Minute30m => "30m",
            Self::Hour1h => "1h",
            Self::Hour4h => "4h",
            Self::Week1w => "1w",
            Self::Day1d => "1d",
            Self::Month1mo => "1mo",
        }
    }

    pub fn all() -> [Self; 11] {
        [
            Self::Tick1s,
            Self::Minute1m,
            Self::Minute3m,
            Self::Minute5m,
            Self::Minute15m,
            Self::Minute30m,
            Self::Hour1h,
            Self::Hour4h,
            Self::Week1w,
            Self::Day1d,
            Self::Month1mo,
        ]
    }

    pub fn from_interval_label(value: &str) -> Option<Self> {
        Some(match value {
            "1s" => Self::Tick1s,
            "1m" => Self::Minute1m,
            "3m" => Self::Minute3m,
            "5m" => Self::Minute5m,
            "15m" => Self::Minute15m,
            "30m" => Self::Minute30m,
            "1h" => Self::Hour1h,
            "4h" => Self::Hour4h,
            "1w" => Self::Week1w,
            "1d" => Self::Day1d,
            "1mo" => Self::Month1mo,
            _ => return None,
        })
    }

    pub fn rank(self) -> usize {
        match self {
            Self::Tick1s => 0,
            Self::Minute1m => 1,
            Self::Minute3m => 2,
            Self::Minute5m => 3,
            Self::Minute15m => 4,
            Self::Minute30m => 5,
            Self::Hour1h => 6,
            Self::Hour4h => 7,
            Self::Week1w => 8,
            Self::Day1d => 9,
            Self::Month1mo => 10,
        }
    }
}

pub fn market_scene_from_snapshot(snapshot: &DashboardSnapshot) -> ChartScene {
    market_scene_from_snapshot_with_timeframe(snapshot, MarketTimeframe::Tick1s)
}

pub fn market_scene_from_snapshot_with_timeframe(
    snapshot: &DashboardSnapshot,
    timeframe: MarketTimeframe,
) -> ChartScene {
    build_market_scene(
        snapshot,
        timeframe,
        MarketSeriesKind::Candles,
        Some(MarketSeriesKind::MidPrice),
        true,
    )
}

pub fn market_scene_from_snapshot_with_overlay(
    snapshot: &DashboardSnapshot,
    timeframe: MarketTimeframe,
    primary: MarketSeriesKind,
    secondary: Option<MarketSeriesKind>,
) -> ChartScene {
    build_market_scene(snapshot, timeframe, primary, secondary, false)
}

fn build_market_scene(
    snapshot: &DashboardSnapshot,
    timeframe: MarketTimeframe,
    primary: MarketSeriesKind,
    secondary: Option<MarketSeriesKind>,
    include_default_annotations: bool,
) -> ChartScene {
    let effective_timeframe = snapshot
        .market_series
        .kline_interval
        .as_deref()
        .and_then(MarketTimeframe::from_interval_label)
        .filter(|source| source.rank() > timeframe.rank())
        .unwrap_or(timeframe);
    let display_klines =
        aggregate_klines_for_timeframe(&snapshot.market_series.klines, effective_timeframe);
    let mut price_series = build_overlay_series(snapshot, &display_klines, primary, false);
    if let Some(secondary) = secondary.filter(|kind| *kind != primary) {
        price_series.extend(build_overlay_series(
            snapshot,
            &display_klines,
            secondary,
            true,
        ));
    }

    if include_default_annotations {
        let mut markers = liquidation_marker_series(snapshot);
        if let Some(report) = snapshot
            .selected_report
            .as_ref()
            .filter(|report| report.instrument == snapshot.symbol)
        {
            markers.extend(
                VisualizationService::signal_markers(&report.trades)
                    .into_iter()
                    .map(|marker| Marker {
                        label: marker.label,
                        time_ms: EpochMs::from(marker.time_ms),
                        value: marker.price,
                        color: signal_color(marker.kind),
                        size: 8,
                        shape: MarkerShape::Cross,
                    }),
            );
        }

        if !markers.is_empty() {
            price_series.push(Series::Markers(MarkerSeries {
                name: "signals".to_string(),
                markers,
            }));
        }
    }

    let mut panes = vec![Pane {
        id: "market".to_string(),
        title: Some(format!("Market ({})", effective_timeframe.label())),
        weight: 4,
        y_axis: usdt_axis(2, false),
        series: price_series,
    }];

    if !display_klines.is_empty() {
        panes.push(Pane {
            id: "volume".to_string(),
            title: Some(format!("Volume ({})", effective_timeframe.label())),
            weight: 1,
            y_axis: compact_axis("Volume", 1, true),
            series: vec![Series::Bars(BarSeries {
                name: "volume".to_string(),
                color: VOLUME_UP,
                bars: display_klines
                    .iter()
                    .map(|row| Bar {
                        open_time_ms: EpochMs::from(row.open_time_ms),
                        close_time_ms: EpochMs::from(row.close_time_ms),
                        value: row.volume,
                        color: Some(if row.close >= row.open {
                            VOLUME_UP
                        } else {
                            VOLUME_DOWN
                        }),
                    })
                    .collect(),
            })],
        });
    }

    ChartScene {
        title: format!(
            "{} | liq {} | ticks {} | {} bars {}",
            snapshot.symbol,
            snapshot.dataset_summary.liquidation_events,
            snapshot.dataset_summary.book_ticker_events,
            effective_timeframe.label(),
            display_klines.len(),
        ),
        time_label_format: "%m-%d %H:%M".to_string(),
        theme: ChartTheme::default(),
        viewport: focused_market_viewport(snapshot),
        hover: Some(
            TooltipModel {
                title: "Market".to_string(),
                sections: Vec::new(),
            }
            .into(),
        ),
        panes,
    }
}

fn focused_market_viewport(snapshot: &DashboardSnapshot) -> Viewport {
    let Some(report) = snapshot
        .selected_report
        .as_ref()
        .filter(|report| report.instrument == snapshot.symbol && !report.trades.is_empty())
    else {
        return Viewport::default();
    };

    let mut min_time = i64::MAX;
    let mut max_time = i64::MIN;
    for trade in &report.trades {
        min_time = min_time.min(trade.entry_time.timestamp_millis());
        max_time = max_time.max(
            trade
                .exit_time
                .map(|value| value.timestamp_millis())
                .unwrap_or_else(|| trade.entry_time.timestamp_millis()),
        );
    }
    if min_time > max_time {
        return Viewport::default();
    }
    let span = (max_time - min_time).max(1);
    if span > 25 * 60 * 1_000 {
        return Viewport {
            x_range: Some((
                EpochMs::new(max_time.saturating_sub(25 * 60 * 1_000)),
                EpochMs::new(max_time.saturating_add(2 * 60 * 1_000)),
            )),
        };
    }
    let padding = ((span as f64) * 0.35).round() as i64;
    let padding = padding.max(5 * 60 * 1_000);
    Viewport {
        x_range: Some((
            EpochMs::new(min_time.saturating_sub(padding)),
            EpochMs::new(max_time.saturating_add(padding)),
        )),
    }
}

pub fn equity_scene_from_report(report: &BacktestReport) -> ChartScene {
    let mut points = VisualizationService::equity_curve(report.starting_equity, &report.trades)
        .into_iter()
        .map(|point| LinePoint {
            time_ms: EpochMs::from(point.time_ms),
            value: point.equity,
        })
        .collect::<Vec<_>>();
    if let Some(first) = points.first().cloned() {
        points.insert(
            0,
            LinePoint {
                time_ms: first.time_ms,
                value: report.starting_equity,
            },
        );
    }
    ChartScene {
        title: format!(
            "Run #{} | ending equity {:.2} | net pnl {:.2}",
            report.run_id.unwrap_or_default(),
            report.ending_equity,
            report.net_pnl,
        ),
        time_label_format: "%m-%d %H:%M".to_string(),
        theme: ChartTheme::default(),
        viewport: Viewport::default(),
        hover: Some(
            TooltipModel {
                title: "Equity".to_string(),
                sections: Vec::new(),
            }
            .into(),
        ),
        panes: vec![Pane {
            id: "equity".to_string(),
            title: Some("Equity".to_string()),
            weight: 1,
            y_axis: usdt_axis(2, false),
            series: vec![Series::Line(LineSeries {
                name: "equity".to_string(),
                color: EQUITY,
                width: 2,
                points,
            })],
        }],
    }
}

fn signal_color(kind: SignalKind) -> RgbColor {
    match kind {
        SignalKind::Entry => ENTRY,
        SignalKind::TakeProfit => TAKE_PROFIT,
        SignalKind::StopLoss => STOP_LOSS,
        SignalKind::OpenAtEnd => OPEN_AT_END,
        SignalKind::SignalExit => SIGNAL_EXIT,
    }
}

fn build_overlay_series(
    snapshot: &DashboardSnapshot,
    display_klines: &[crate::dataset::types::DerivedKlineRow],
    kind: MarketSeriesKind,
    secondary: bool,
) -> Vec<Series> {
    match kind {
        MarketSeriesKind::Candles if !display_klines.is_empty() => {
            vec![Series::Candles(CandleSeries {
                name: if secondary {
                    "candles-secondary".to_string()
                } else {
                    "candles".to_string()
                },
                up_color: None,
                down_color: None,
                candles: display_klines
                    .iter()
                    .map(|row| Candle {
                        open_time_ms: EpochMs::from(row.open_time_ms),
                        close_time_ms: EpochMs::from(row.close_time_ms),
                        open: row.open,
                        high: row.high,
                        low: row.low,
                        close: row.close,
                    })
                    .collect(),
            })]
        }
        MarketSeriesKind::MidPrice => {
            sampled_mid_price_segments(snapshot, 2_400, 60_000, if secondary { 2 } else { 1 })
        }
        MarketSeriesKind::CloseLine if !display_klines.is_empty() => {
            vec![Series::Line(LineSeries {
                name: if secondary {
                    "close-secondary".to_string()
                } else {
                    "close".to_string()
                },
                color: if secondary { SECONDARY_LINE } else { PRICE },
                width: if secondary { 2 } else { 1 },
                points: display_klines
                    .iter()
                    .map(|row| LinePoint {
                        time_ms: EpochMs::from(row.close_time_ms),
                        value: row.close,
                    })
                    .collect(),
            })]
        }
        MarketSeriesKind::Ema20 if !display_klines.is_empty() => {
            let points = ema_points(display_klines, 20);
            if points.is_empty() {
                Vec::new()
            } else {
                vec![Series::Line(LineSeries {
                    name: if secondary {
                        "ema20-secondary".to_string()
                    } else {
                        "ema20".to_string()
                    },
                    color: if secondary { SECONDARY_LINE } else { PRICE },
                    width: 2,
                    points,
                })]
            }
        }
        MarketSeriesKind::Vwap if !display_klines.is_empty() => {
            let points = vwap_points(display_klines);
            if points.is_empty() {
                Vec::new()
            } else {
                vec![Series::Line(LineSeries {
                    name: if secondary {
                        "vwap-secondary".to_string()
                    } else {
                        "vwap".to_string()
                    },
                    color: if secondary { SECONDARY_LINE } else { PRICE },
                    width: 2,
                    points,
                })]
            }
        }
        MarketSeriesKind::Liquidations => {
            let markers = liquidation_marker_series(snapshot);
            if markers.is_empty() {
                Vec::new()
            } else {
                vec![Series::Markers(MarkerSeries {
                    name: if secondary {
                        "liquidations-secondary".to_string()
                    } else {
                        "liquidations".to_string()
                    },
                    markers,
                })]
            }
        }
        _ => Vec::new(),
    }
}

fn liquidation_marker_series(snapshot: &DashboardSnapshot) -> Vec<Marker> {
    snapshot
        .market_series
        .liquidations
        .iter()
        .map(|row| Marker {
            label: row.force_side.clone(),
            time_ms: EpochMs::from(row.event_time_ms),
            value: row.price,
            color: if row.force_side == "BUY" {
                LIQ_BUY
            } else {
                LIQ_OTHER
            },
            size: ((row.notional.max(1.0)).log10().clamp(0.0, 7.0) as i32) + 3,
            shape: MarkerShape::Circle,
        })
        .collect()
}

fn ema_points(
    display_klines: &[crate::dataset::types::DerivedKlineRow],
    period: usize,
) -> Vec<LinePoint> {
    if display_klines.is_empty() {
        return Vec::new();
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut ema = display_klines[0].close;
    display_klines
        .iter()
        .map(|row| {
            ema = row.close * alpha + ema * (1.0 - alpha);
            LinePoint {
                time_ms: EpochMs::from(row.close_time_ms),
                value: ema,
            }
        })
        .collect()
}

fn vwap_points(display_klines: &[crate::dataset::types::DerivedKlineRow]) -> Vec<LinePoint> {
    let mut cumulative_quote = 0.0;
    let mut cumulative_volume = 0.0;
    let mut points = Vec::new();
    for row in display_klines {
        cumulative_quote += row.quote_volume;
        cumulative_volume += row.volume.max(f64::EPSILON);
        points.push(LinePoint {
            time_ms: EpochMs::from(row.close_time_ms),
            value: cumulative_quote / cumulative_volume,
        });
    }
    points
}

fn usdt_axis(decimals: u8, include_zero: bool) -> YAxisSpec {
    YAxisSpec {
        label: Some("USDT".to_string()),
        formatter: ValueFormatter::Number {
            decimals,
            prefix: String::new(),
            suffix: " USDT".to_string(),
        },
        include_zero,
    }
}

fn compact_axis(label: &str, decimals: u8, include_zero: bool) -> YAxisSpec {
    YAxisSpec {
        label: Some(label.to_string()),
        formatter: ValueFormatter::Compact {
            decimals,
            prefix: String::new(),
            suffix: String::new(),
        },
        include_zero,
    }
}

fn sampled_mid_price_segments(
    snapshot: &DashboardSnapshot,
    max_points: usize,
    max_gap_ms: i64,
    width: u32,
) -> Vec<Series> {
    let prices = VisualizationService::price_points(&snapshot.market_series);
    if prices.is_empty() {
        return Vec::new();
    }
    let stride = if prices.len() <= max_points || max_points == 0 {
        1
    } else {
        (prices.len() / max_points).max(1)
    };
    let sampled = prices
        .into_iter()
        .step_by(stride)
        .map(|point| LinePoint {
            time_ms: EpochMs::from(point.time_ms),
            value: point.price,
        })
        .collect::<Vec<_>>();
    if sampled.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut current = Vec::new();
    for point in sampled {
        let gap_too_large = current.last().is_some_and(|last: &LinePoint| {
            point.time_ms.as_i64() - last.time_ms.as_i64() > max_gap_ms
        });
        if gap_too_large && current.len() >= 2 {
            segments.push(Series::Line(LineSeries {
                name: "mid-price".to_string(),
                color: PRICE,
                width,
                points: std::mem::take(&mut current),
            }));
        }
        current.push(point);
    }
    if current.len() >= 2 {
        segments.push(Series::Line(LineSeries {
            name: "mid-price".to_string(),
            color: PRICE,
            width,
            points: current,
        }));
    }
    segments
}

fn aggregate_klines_for_timeframe(
    klines: &[crate::dataset::types::DerivedKlineRow],
    timeframe: MarketTimeframe,
) -> Vec<crate::dataset::types::DerivedKlineRow> {
    match timeframe {
        MarketTimeframe::Tick1s => klines.to_vec(),
        MarketTimeframe::Minute1m => aggregate_klines(klines, bucket_start_minute),
        MarketTimeframe::Minute3m => aggregate_klines(klines, |ms| bucket_start_minutes(ms, 3)),
        MarketTimeframe::Minute5m => aggregate_klines(klines, |ms| bucket_start_minutes(ms, 5)),
        MarketTimeframe::Minute15m => aggregate_klines(klines, |ms| bucket_start_minutes(ms, 15)),
        MarketTimeframe::Minute30m => aggregate_klines(klines, |ms| bucket_start_minutes(ms, 30)),
        MarketTimeframe::Hour1h => aggregate_klines(klines, |ms| bucket_start_hours(ms, 1)),
        MarketTimeframe::Hour4h => aggregate_klines(klines, |ms| bucket_start_hours(ms, 4)),
        MarketTimeframe::Week1w => aggregate_klines(klines, bucket_start_week),
        MarketTimeframe::Day1d => aggregate_klines(klines, bucket_start_day),
        MarketTimeframe::Month1mo => aggregate_klines(klines, bucket_start_month),
    }
}

fn aggregate_klines(
    klines: &[crate::dataset::types::DerivedKlineRow],
    bucket_start: fn(i64) -> i64,
) -> Vec<crate::dataset::types::DerivedKlineRow> {
    if klines.is_empty() {
        return Vec::new();
    }
    let mut aggregated = Vec::new();
    let mut current_bucket = bucket_start(klines[0].open_time_ms);
    let mut current = klines[0].clone();
    current.open_time_ms = current_bucket;

    for row in klines.iter().skip(1) {
        let next_bucket = bucket_start(row.open_time_ms);
        if next_bucket != current_bucket {
            aggregated.push(current);
            current_bucket = next_bucket;
            current = row.clone();
            current.open_time_ms = current_bucket;
            continue;
        }
        current.close_time_ms = row.close_time_ms;
        current.high = current.high.max(row.high);
        current.low = current.low.min(row.low);
        current.close = row.close;
        current.volume += row.volume;
        current.quote_volume += row.quote_volume;
        current.trade_count += row.trade_count;
    }
    aggregated.push(current);
    aggregated
}

fn bucket_start_minute(ms: i64) -> i64 {
    (ms / 60_000) * 60_000
}

fn bucket_start_minutes(ms: i64, minutes: i64) -> i64 {
    let bucket_ms = minutes * 60_000;
    (ms / bucket_ms) * bucket_ms
}

fn bucket_start_hours(ms: i64, hours: i64) -> i64 {
    let bucket_ms = hours * 60 * 60_000;
    (ms / bucket_ms) * bucket_ms
}

fn bucket_start_day(ms: i64) -> i64 {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .and_then(|dt| {
            dt.date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|naive| naive.and_utc().timestamp_millis())
        })
        .unwrap_or(ms)
}

fn bucket_start_week(ms: i64) -> i64 {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .and_then(|dt| {
            let date = dt.date_naive();
            let weekday_offset = i64::from(date.weekday().num_days_from_monday());
            date.checked_sub_days(chrono::Days::new(weekday_offset as u64))
                .and_then(|start| start.and_hms_opt(0, 0, 0))
                .map(|naive| naive.and_utc().timestamp_millis())
        })
        .unwrap_or(ms)
}

fn bucket_start_month(ms: i64) -> i64 {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .and_then(|dt| {
            chrono::NaiveDate::from_ymd_opt(dt.year(), dt.month(), 1)
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|naive| naive.and_utc().timestamp_millis())
        })
        .unwrap_or(ms)
}

impl From<TooltipModel> for HoverModel {
    fn from(tooltip: TooltipModel) -> Self {
        Self {
            crosshair: None,
            tooltip: Some(tooltip),
        }
    }
}
