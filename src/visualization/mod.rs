pub mod service;
pub mod types;

pub use service::VisualizationService;
pub use types::{
    BacktestRunRequest, DashboardQuery, DashboardSnapshot, EquityPoint, MarketSeries, PricePoint,
    SignalKind, SignalMarker,
};
