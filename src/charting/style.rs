#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartTheme {
    pub background: RgbColor,
    pub grid: RgbColor,
    pub axis: RgbColor,
    pub text: RgbColor,
    pub bull_candle: RgbColor,
    pub bear_candle: RgbColor,
}

impl Default for ChartTheme {
    fn default() -> Self {
        Self {
            background: RgbColor::new(16, 20, 24),
            grid: RgbColor::new(46, 54, 62),
            axis: RgbColor::new(170, 176, 184),
            text: RgbColor::new(220, 225, 230),
            bull_candle: RgbColor::new(84, 208, 136),
            bear_candle: RgbColor::new(255, 110, 110),
        }
    }
}
