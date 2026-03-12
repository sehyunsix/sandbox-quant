use crate::app::bootstrap::BinanceMode;
use crate::dataset::types::BacktestDatasetSummary;
use crate::record::manager::format_mode;
use crate::strategy::model::StrategyTemplate;

pub fn render_backtest_run(
    template: StrategyTemplate,
    instrument: &str,
    mode: BinanceMode,
    db_path: &std::path::Path,
    summary: &BacktestDatasetSummary,
) -> String {
    [
        "backtest run".to_string(),
        format!("mode={}", format_mode(mode)),
        format!("template={}", template.slug()),
        format!("instrument={}", instrument),
        format!("from={}", summary.from),
        format!("to={}", summary.to),
        format!("db_path={}", db_path.display()),
        format!("liquidation_events={}", summary.liquidation_events),
        format!("book_ticker_events={}", summary.book_ticker_events),
        format!("agg_trade_events={}", summary.agg_trade_events),
        format!("derived_kline_1s_bars={}", summary.derived_kline_1s_bars),
        "outcome=dataset-ready".to_string(),
    ]
    .join("\n")
}
