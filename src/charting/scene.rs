use crate::charting::style::{ChartTheme, RgbColor};

/// Millisecond timestamp wrapper kept backend-neutral for future extractability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EpochMs(i64);

impl EpochMs {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl From<i64> for EpochMs {
    fn from(value: i64) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderRequest {
    pub width_px: u32,
    pub height_px: u32,
    pub pixel_ratio: f32,
    pub oversample: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedFrame {
    pub width_px: u32,
    pub height_px: u32,
    pub rgb: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartScene {
    pub title: String,
    pub time_label_format: String,
    pub theme: ChartTheme,
    pub viewport: Viewport,
    /// Optional shared hover/crosshair state that backends may visualize.
    pub hover: Option<HoverModel>,
    pub panes: Vec<Pane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Viewport {
    pub x_range: Option<(EpochMs, EpochMs)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pane {
    pub id: String,
    pub title: Option<String>,
    /// Relative vertical weight used when a scene is split into stacked panes.
    pub weight: u16,
    pub y_axis: YAxisSpec,
    pub series: Vec<Series>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct YAxisSpec {
    pub label: Option<String>,
    pub formatter: ValueFormatter,
    pub include_zero: bool,
}

impl Default for YAxisSpec {
    fn default() -> Self {
        Self {
            label: None,
            formatter: ValueFormatter::Number {
                decimals: 2,
                prefix: String::new(),
                suffix: String::new(),
            },
            include_zero: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueFormatter {
    Number {
        decimals: u8,
        prefix: String,
        suffix: String,
    },
    Compact {
        decimals: u8,
        prefix: String,
        suffix: String,
    },
    Percent {
        decimals: u8,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Series {
    Candles(CandleSeries),
    Bars(BarSeries),
    Line(LineSeries),
    Markers(MarkerSeries),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandleSeries {
    pub name: String,
    pub up_color: Option<RgbColor>,
    pub down_color: Option<RgbColor>,
    pub candles: Vec<Candle>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candle {
    pub open_time_ms: EpochMs,
    pub close_time_ms: EpochMs,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarSeries {
    pub name: String,
    pub color: RgbColor,
    pub bars: Vec<Bar>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub open_time_ms: EpochMs,
    pub close_time_ms: EpochMs,
    pub value: f64,
    pub color: Option<RgbColor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineSeries {
    pub name: String,
    pub color: RgbColor,
    pub width: u32,
    pub points: Vec<LinePoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinePoint {
    pub time_ms: EpochMs,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkerSeries {
    pub name: String,
    pub markers: Vec<Marker>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub label: String,
    pub time_ms: EpochMs,
    pub value: f64,
    pub color: RgbColor,
    pub size: i32,
    pub shape: MarkerShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerShape {
    Circle,
    Cross,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HoverModel {
    pub crosshair: Option<Crosshair>,
    pub tooltip: Option<TooltipModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Crosshair {
    pub time_ms: EpochMs,
    pub value: Option<f64>,
    pub color: Option<RgbColor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TooltipModel {
    pub title: String,
    pub sections: Vec<TooltipSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TooltipSection {
    pub title: String,
    pub rows: Vec<TooltipRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TooltipRow {
    pub label: String,
    pub value: String,
}
