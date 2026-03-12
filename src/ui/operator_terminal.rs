use crossterm::style::{Color, Stylize};

use crate::app::bootstrap::BinanceMode;
use crate::portfolio::store::PortfolioStateStore;

pub fn shell_intro_panel(mode: &str, directory: &str) -> String {
    let width = 46usize;
    let title = format!(" >_ Sandbox Quant (v{})", env!("CARGO_PKG_VERSION"));
    let mode_line = format!(" mode:      {mode:<18} /mode to change");
    let dir_line = format!(" directory: {directory}");

    format!(
        "╭{top}╮\n│{title:<width$}│\n│{blank:<width$}│\n│{mode_line:<width$}│\n│{dir_line:<width$}│\n╰{top}╯",
        top = "─".repeat(width),
        title = title,
        blank = "",
        mode_line = mode_line,
        dir_line = dir_line,
        width = width,
    )
}

pub fn mode_name(mode: BinanceMode) -> &'static str {
    match mode {
        BinanceMode::Real => "real",
        BinanceMode::Demo => "demo",
    }
}

pub fn mode_color(mode: BinanceMode) -> Color {
    match mode {
        BinanceMode::Real => Color::Green,
        BinanceMode::Demo => Color::Yellow,
    }
}

pub fn prompt_status_from_store(store: &PortfolioStateStore) -> String {
    let position_count = store
        .snapshot
        .positions
        .values()
        .filter(|position| !position.is_flat())
        .count();
    let open_order_count: usize = store.snapshot.open_orders.values().map(Vec::len).sum();
    format!(
        "[{}|{} pos|{} ord]",
        staleness_label(store.staleness),
        position_count,
        open_order_count,
    )
}

pub fn operator_prompt(mode: BinanceMode, status: &str) -> String {
    format!(
        "{} [{}] {} › ",
        "●".with(mode_color(mode)),
        mode_name(mode),
        status
    )
}

fn staleness_label(staleness: crate::portfolio::staleness::StalenessState) -> &'static str {
    match staleness {
        crate::portfolio::staleness::StalenessState::Fresh => "fresh",
        crate::portfolio::staleness::StalenessState::MarketDataStale => "market-stale",
        crate::portfolio::staleness::StalenessState::AccountStateStale => "account-stale",
        crate::portfolio::staleness::StalenessState::ReconciliationStale => "reconcile-stale",
    }
}
