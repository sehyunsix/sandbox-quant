use eframe::egui::{self, vec2, Color32, ComboBox, Grid, RichText, SidePanel, TopBottomPanel, Ui};

use crate::app::bootstrap::BinanceMode;
use crate::backtest_app::runner::{BacktestConfig, BacktestReport};
use crate::charting::adapters::sandbox::{
    equity_scene_from_report, market_scene_from_snapshot_with_timeframe, MarketTimeframe,
};
use crate::charting::egui::RetainedChartTexture;
use crate::charting::inspect::{hover_model_at, pan_scene, visible_time_bounds, zoom_scene};
use crate::charting::plotters::PlottersRenderer;
use crate::charting::render::ChartRenderer;
use crate::charting::scene::{ChartScene, RenderRequest, TooltipModel, Viewport};
use crate::strategy::model::StrategyTemplate;
use crate::visualization::service::VisualizationService;
use crate::visualization::types::{BacktestRunRequest, DashboardQuery, DashboardSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub struct GuiLaunchConfig {
    pub mode: BinanceMode,
    pub base_dir: String,
    pub symbol: String,
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub market_timeframe: MarketTimeframe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuiTab {
    Overview,
    Market,
    Pnl,
    Trades,
}

impl GuiTab {
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Market => "Market",
            Self::Pnl => "PnL",
            Self::Trades => "Trades",
        }
    }

    fn all() -> [Self; 4] {
        [Self::Overview, Self::Market, Self::Pnl, Self::Trades]
    }
}

pub struct SandboxQuantGuiApp {
    service: VisualizationService,
    mode: BinanceMode,
    template: StrategyTemplate,
    base_dir_input: String,
    symbol_input: String,
    from_input: String,
    to_input: String,
    run_limit: usize,
    market_timeframe: MarketTimeframe,
    selected_tab: GuiTab,
    snapshot: Option<DashboardSnapshot>,
    status_message: String,
    market_chart: RetainedChartTexture,
    equity_chart: RetainedChartTexture,
    market_viewport: Viewport,
    equity_viewport: Viewport,
}

impl SandboxQuantGuiApp {
    pub fn new(launch: GuiLaunchConfig) -> Self {
        let mut app = Self {
            service: VisualizationService,
            mode: launch.mode,
            template: StrategyTemplate::LiquidationBreakdownShort,
            base_dir_input: launch.base_dir,
            symbol_input: launch.symbol,
            from_input: launch.from.to_string(),
            to_input: launch.to.to_string(),
            run_limit: 24,
            market_timeframe: launch.market_timeframe,
            selected_tab: GuiTab::Overview,
            snapshot: None,
            status_message: "Ready".to_string(),
            market_chart: RetainedChartTexture::default(),
            equity_chart: RetainedChartTexture::default(),
            market_viewport: Viewport::default(),
            equity_viewport: Viewport::default(),
        };
        app.refresh_dashboard(None);
        app
    }

    fn reset_viewports(&mut self) {
        self.market_chart.clear();
        self.equity_chart.clear();
        self.market_viewport = Viewport::default();
        self.equity_viewport = Viewport::default();
    }

    fn apply_today_preset(&mut self) {
        let today = chrono::Utc::now().date_naive();
        self.from_input = today.to_string();
        self.to_input = today.to_string();
    }

    fn apply_last_days_preset(&mut self, days: u64) {
        let today = chrono::Utc::now().date_naive();
        self.from_input = (today - chrono::Days::new(days.saturating_sub(1))).to_string();
        self.to_input = today.to_string();
    }

    fn refresh_dashboard(&mut self, selected_run_id: Option<i64>) {
        self.refresh_dashboard_with_fallback(selected_run_id, true);
    }

    fn refresh_dashboard_with_fallback(
        &mut self,
        selected_run_id: Option<i64>,
        allow_fallback: bool,
    ) {
        match self.dashboard_query(selected_run_id) {
            Ok(query) => match self.service.load_dashboard(query) {
                Ok(snapshot) => {
                    if allow_fallback
                        && snapshot.market_series.book_tickers.is_empty()
                        && snapshot.market_series.klines.is_empty()
                        && snapshot.market_series.liquidations.is_empty()
                    {
                        if let Some(latest_available_day) = self
                            .service
                            .latest_market_data_day(
                                self.mode,
                                self.base_dir_input.trim().into(),
                                self.symbol_input.trim(),
                            )
                            .ok()
                            .flatten()
                            .or_else(|| latest_available_data_day(&snapshot.recorder_metrics))
                        {
                            let latest_text = latest_available_day.to_string();
                            if self.from_input != latest_text || self.to_input != latest_text {
                                self.from_input = latest_text.clone();
                                self.to_input = latest_text;
                                self.status_message = format!(
                                    "No data in requested range. Falling back to latest available UTC day {}.",
                                    latest_available_day
                                );
                                self.refresh_dashboard_with_fallback(selected_run_id, false);
                                return;
                            }
                        }
                    }
                    if self.symbol_input.trim().is_empty() && !snapshot.symbol.is_empty() {
                        self.symbol_input = snapshot.symbol.clone();
                    }
                    let mut status_message = format!(
                        "Loaded {} | symbol={} | {} ticks | {} liqs | {} runs",
                        snapshot.db_path.display(),
                        if snapshot.symbol.is_empty() {
                            "-"
                        } else {
                            snapshot.symbol.as_str()
                        },
                        snapshot.dataset_summary.book_ticker_events,
                        snapshot.dataset_summary.liquidation_events,
                        snapshot.recent_runs.len(),
                    );
                    if let Some(source_interval) =
                        source_interval_hint(&snapshot, self.market_timeframe)
                    {
                        status_message.push_str(&format!(" | source={source_interval}"));
                    }
                    self.status_message = status_message;
                    self.snapshot = Some(snapshot);
                    self.reset_viewports();
                }
                Err(error) => {
                    self.status_message = format!("Load failed: {error}");
                    self.snapshot = None;
                }
            },
            Err(error) => {
                self.status_message = error;
            }
        }
    }

    fn run_backtest(&mut self) {
        let from = match parse_date("from", &self.from_input) {
            Ok(value) => value,
            Err(error) => {
                self.status_message = error;
                return;
            }
        };
        let to = match parse_date("to", &self.to_input) {
            Ok(value) => value,
            Err(error) => {
                self.status_message = error;
                return;
            }
        };
        let symbol = self.symbol_input.trim().to_ascii_uppercase();
        if symbol.is_empty() {
            self.status_message = "symbol is required".to_string();
            return;
        }
        match self.service.run_backtest(BacktestRunRequest {
            mode: self.mode,
            base_dir: self.base_dir_input.trim().into(),
            symbol,
            from,
            to,
            template: self.template,
            config: BacktestConfig::default(),
            run_limit: self.run_limit,
        }) {
            Ok(snapshot) => {
                self.status_message = format!(
                    "Backtest complete: net_pnl={:.2} ending_equity={:.2}",
                    snapshot
                        .selected_report
                        .as_ref()
                        .map(|report| report.net_pnl)
                        .unwrap_or_default(),
                    snapshot
                        .selected_report
                        .as_ref()
                        .map(|report| report.ending_equity)
                        .unwrap_or_default(),
                );
                self.snapshot = Some(snapshot);
                self.reset_viewports();
                self.selected_tab = GuiTab::Pnl;
            }
            Err(error) => {
                self.status_message = format!("Backtest failed: {error}");
            }
        }
    }

    fn dashboard_query(&self, selected_run_id: Option<i64>) -> Result<DashboardQuery, String> {
        Ok(DashboardQuery {
            mode: self.mode,
            base_dir: self.base_dir_input.trim().into(),
            symbol: self.symbol_input.trim().to_ascii_uppercase(),
            from: parse_date("from", &self.from_input)?,
            to: parse_date("to", &self.to_input)?,
            selected_run_id,
            run_limit: self.run_limit,
        })
    }

    fn render_overview(&mut self, ui: &mut Ui, snapshot: &DashboardSnapshot) {
        ui.heading(format!("{} | {}", snapshot.mode.as_str(), snapshot.symbol));
        ui.label(format!(
            "{} -> {} | liqs {} | ticks {} | klines {}",
            snapshot.from,
            snapshot.to,
            snapshot.dataset_summary.liquidation_events,
            snapshot.dataset_summary.book_ticker_events,
            snapshot.dataset_summary.derived_kline_1s_bars,
        ));
        ui.add_space(8.0);
        self.show_market_chart(ui, snapshot, 320.0);
        ui.add_space(8.0);
        if let Some(report) = &snapshot.selected_report {
            ui.group(|ui| {
                ui.heading("Selected Run");
                Grid::new("selected_run").num_columns(2).show(ui, |ui| {
                    ui.label("Run ID");
                    ui.strong(report.run_id.unwrap_or_default().to_string());
                    ui.end_row();
                    ui.label("Net PnL");
                    ui.strong(format!("{:.2}", report.net_pnl));
                    ui.end_row();
                    ui.label("Ending Equity");
                    ui.strong(format!("{:.2}", report.ending_equity));
                    ui.end_row();
                    ui.label("Win Rate");
                    ui.strong(format!("{:.1}%", report.observed_win_rate * 100.0));
                    ui.end_row();
                    ui.label("Trades");
                    ui.strong(report.trades.len().to_string());
                    ui.end_row();
                });
            });
        }
    }

    fn render_market(&mut self, ui: &mut Ui, snapshot: &DashboardSnapshot) {
        if let Some(source_interval) = source_interval_hint(snapshot, self.market_timeframe) {
            ui.label(
                RichText::new(format!(
                    "Requested {} view, but stored source is {}. Rendering from the coarser source.",
                    self.market_timeframe.label(),
                    source_interval
                ))
                .color(Color32::from_rgb(255, 210, 120))
                .strong(),
            );
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Candlestick / time axis")
                    .color(Color32::from_rgb(120, 220, 180))
                    .strong(),
            );
            ui.label(
                RichText::new("Liquidation overlays")
                    .color(Color32::from_rgb(255, 140, 90))
                    .strong(),
            );
            ui.label(
                RichText::new("Entry / exit signals")
                    .color(Color32::from_rgb(120, 180, 255))
                    .strong(),
            );
        });
        self.show_market_chart(ui, snapshot, 520.0);
    }

    fn render_pnl(&mut self, ui: &mut Ui, snapshot: &DashboardSnapshot) {
        let Some(report) = &snapshot.selected_report else {
            ui.label("No backtest report selected.");
            return;
        };
        ui.heading(format!(
            "PnL | run #{} | {} trades",
            report.run_id.unwrap_or_default(),
            report.trades.len()
        ));
        ui.label(format!(
            "starting {:.2} -> ending {:.2} | avg {:.2}",
            report.starting_equity, report.ending_equity, report.average_net_pnl
        ));
        self.show_equity_chart(ui, report, 360.0);
        ui.add_space(8.0);
        Grid::new("pnl_summary").num_columns(4).show(ui, |ui| {
            ui.label("Wins");
            ui.strong(report.wins.to_string());
            ui.label("Losses");
            ui.strong(report.losses.to_string());
            ui.end_row();
            ui.label("Open");
            ui.strong(report.open_trades.to_string());
            ui.label("Skipped");
            ui.strong(report.skipped_triggers.to_string());
            ui.end_row();
        });
    }

    fn show_market_chart(&mut self, ui: &mut Ui, snapshot: &DashboardSnapshot, height: f32) {
        if snapshot.market_series.klines.is_empty()
            && snapshot.market_series.book_tickers.is_empty()
            && snapshot.market_series.liquidations.is_empty()
        {
            ui.group(|ui| {
                ui.label(
                    RichText::new("No market data for the current filters.")
                        .color(Color32::from_rgb(255, 210, 120))
                        .strong(),
                );
                ui.label("Try a different symbol/date range or import/load more data.");
            });
            return;
        }
        let size = vec2(ui.available_width().max(320.0), height);
        let request = render_request(ui, size);
        let renderer = PlottersRenderer;
        let mut scene = market_scene_from_snapshot_with_timeframe(snapshot, self.market_timeframe);
        if self.market_viewport.x_range.is_some() {
            scene.viewport = self.market_viewport.clone();
        }
        let interval_label = market_period_unit_label(snapshot, self.market_timeframe);
        render_chart_period_label(ui, &scene, &interval_label);
        match renderer.render(&scene, &request) {
            Ok(frame) => {
                self.market_chart.update(ui.ctx(), "market-chart", &frame);
                if let Some(response) = self.market_chart.show(ui, size) {
                    apply_hover(
                        ui,
                        &renderer,
                        &mut scene,
                        &request,
                        ChartInteraction {
                            response,
                            texture: &mut self.market_chart,
                            viewport: &mut self.market_viewport,
                        },
                    );
                }
            }
            Err(error) => {
                ui.colored_label(Color32::from_rgb(255, 120, 120), error.to_string());
            }
        }
    }

    fn show_equity_chart(&mut self, ui: &mut Ui, report: &BacktestReport, height: f32) {
        if report.trades.is_empty() {
            ui.group(|ui| {
                ui.label(
                    RichText::new("No realized trades to chart yet.")
                        .color(Color32::from_rgb(255, 210, 120))
                        .strong(),
                );
                ui.label("Run a strategy with matching data or choose a different backtest run.");
            });
            return;
        }
        let size = vec2(ui.available_width().max(320.0), height);
        let request = render_request(ui, size);
        let renderer = PlottersRenderer;
        let mut scene = equity_scene_from_report(report);
        if self.equity_viewport.x_range.is_some() {
            scene.viewport = self.equity_viewport.clone();
        }
        render_chart_period_label(ui, &scene, "realized equity");
        match renderer.render(&scene, &request) {
            Ok(frame) => {
                self.equity_chart.update(ui.ctx(), "equity-chart", &frame);
                if let Some(response) = self.equity_chart.show(ui, size) {
                    apply_hover(
                        ui,
                        &renderer,
                        &mut scene,
                        &request,
                        ChartInteraction {
                            response,
                            texture: &mut self.equity_chart,
                            viewport: &mut self.equity_viewport,
                        },
                    );
                }
            }
            Err(error) => {
                ui.colored_label(Color32::from_rgb(255, 120, 120), error.to_string());
            }
        }
    }
}

impl eframe::App for SandboxQuantGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Sandbox Quant GUI");
                ui.label(
                    RichText::new("plotters-backed candlesticks, signals, and pnl")
                        .color(Color32::from_rgb(120, 140, 160)),
                );
                ui.separator();
                ui.label(
                    RichText::new(self.status_message.as_str())
                        .color(Color32::from_rgb(180, 220, 255)),
                );
            });
        });

        SidePanel::left("controls")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                let mut selected_symbol = None::<String>;
                let mut selected_run_id = None::<i64>;
                let mut apply_filters = false;

                ui.heading("Dataset Filters");
                Grid::new("top_controls").num_columns(2).show(ui, |ui| {
                    ui.label("Mode");
                    ComboBox::from_id_salt("mode_combo")
                        .selected_text(self.mode.as_str())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.mode, BinanceMode::Demo, "demo");
                            ui.selectable_value(&mut self.mode, BinanceMode::Real, "real");
                        });
                    ui.end_row();

                    ui.label("Timeframe");
                    ComboBox::from_id_salt("timeframe_combo")
                        .selected_text(self.market_timeframe.label())
                        .show_ui(ui, |ui| {
                            for timeframe in MarketTimeframe::all() {
                                ui.selectable_value(
                                    &mut self.market_timeframe,
                                    timeframe,
                                    timeframe.label(),
                                );
                            }
                        });
                    ui.end_row();
                });
                ui.small(
                    RichText::new("Date filters use UTC day boundaries.")
                        .color(Color32::from_rgb(150, 170, 190)),
                );
                if let Some(snapshot) = &self.snapshot {
                    if let Some(source_interval) =
                        source_interval_hint(snapshot, self.market_timeframe)
                    {
                        ui.add_space(4.0);
                        ui.group(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Requested {} • Source {}",
                                    self.market_timeframe.label(),
                                    source_interval
                                ))
                                .color(Color32::from_rgb(255, 210, 120))
                                .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.small("Quick switch:");
                                for timeframe in recommended_timeframes_from_source(&source_interval)
                                {
                                    if ui.small_button(timeframe.label()).clicked() {
                                        self.market_timeframe = timeframe;
                                    }
                                }
                            });
                        });
                    }
                }
                ui.separator();

                if let Some(snapshot) = &self.snapshot {
                    ui.horizontal(|ui| {
                        ui.label("Symbol");
                        ComboBox::from_id_salt("symbol_combo")
                            .selected_text(if self.symbol_input.trim().is_empty() {
                                "select symbol"
                            } else {
                                self.symbol_input.as_str()
                            })
                            .show_ui(ui, |ui| {
                                for symbol in &snapshot.available_symbols {
                                    let selected = self.symbol_input.eq_ignore_ascii_case(symbol);
                                    if ui.selectable_label(selected, symbol).clicked() {
                                        selected_symbol = Some(symbol.clone());
                                    }
                                }
                            });
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Symbol");
                        let symbol_response = ui.add_sized(
                            [180.0, 22.0],
                            egui::TextEdit::singleline(&mut self.symbol_input),
                        );
                        if symbol_response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            apply_filters = true;
                        }
                    });
                }

                ui.horizontal(|ui| {
                    ui.label("From");
                    let from_response =
                        ui.add_sized([110.0, 22.0], egui::TextEdit::singleline(&mut self.from_input));
                    ui.add_space(8.0);
                    ui.label("To");
                    let to_response =
                        ui.add_sized([110.0, 22.0], egui::TextEdit::singleline(&mut self.to_input));
                    if (from_response.lost_focus() || to_response.lost_focus())
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        apply_filters = true;
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    if ui.button("Today").clicked() {
                        self.apply_today_preset();
                        apply_filters = true;
                    }
                    if ui.button("Last 2D").clicked() {
                        self.apply_last_days_preset(2);
                        apply_filters = true;
                    }
                    if ui.button("Last 7D").clicked() {
                        self.apply_last_days_preset(7);
                        apply_filters = true;
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Load Chart").clicked() {
                        apply_filters = true;
                    }
                    if ui.button("Latest Run").clicked() {
                        self.refresh_dashboard(None);
                    }
                    if ui.button("Run Selected Strategy").clicked() {
                        self.run_backtest();
                    }
                    if ui.button("Reset Zoom").clicked() {
                        self.reset_viewports();
                        self.status_message = "Chart zoom/pan reset".to_string();
                    }
                });
                ui.small("Enter applies typed filters. Wheel zooms. Drag pans. Double-click resets viewport.");
                ui.separator();

                if let Some(snapshot) = &self.snapshot {
                    render_metric_cards(ui, snapshot);
                    ui.separator();
                    ui.collapsing("Recent Backtests", |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for row in &snapshot.recent_runs {
                                    let selected = snapshot.selected_run_id == Some(row.run_id);
                                    let label = format!(
                                        "#{} {} pnl {:.2}",
                                        row.run_id, row.template, row.net_pnl
                                    );
                                    if ui.selectable_label(selected, label).clicked() {
                                        selected_run_id = Some(row.run_id);
                                    }
                                }
                            });
                    });
                    ui.collapsing("Advanced", |ui| {
                        ui.label("Base Dir");
                        ui.text_edit_singleline(&mut self.base_dir_input);
                        ui.label("Template");
                        ComboBox::from_id_salt("template_combo")
                            .selected_text(self.template.slug())
                            .show_ui(ui, |ui| {
                                for template in StrategyTemplate::all() {
                                    ui.selectable_value(
                                        &mut self.template,
                                        template,
                                        template.slug(),
                                    );
                                }
                            });
                    });
                } else {
                    ui.label("No snapshot loaded yet.");
                    ui.collapsing("Advanced", |ui| {
                        ui.label("Base Dir");
                        ui.text_edit_singleline(&mut self.base_dir_input);
                        ui.label("Template");
                        ComboBox::from_id_salt("template_combo_empty")
                            .selected_text(self.template.slug())
                            .show_ui(ui, |ui| {
                                for template in StrategyTemplate::all() {
                                    ui.selectable_value(
                                        &mut self.template,
                                        template,
                                        template.slug(),
                                    );
                                }
                            });
                    });
                }

                if let Some(symbol) = selected_symbol {
                    self.symbol_input = symbol;
                    apply_filters = true;
                }
                if let Some(run_id) = selected_run_id {
                    self.refresh_dashboard(Some(run_id));
                } else if apply_filters {
                    self.refresh_dashboard(None);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tab in GuiTab::all() {
                    ui.selectable_value(&mut self.selected_tab, tab, tab.title());
                }
            });
            ui.separator();

            let Some(snapshot) = self.snapshot.clone() else {
                ui.group(|ui| {
                    ui.label(
                        RichText::new("No dashboard snapshot loaded.")
                            .color(Color32::from_rgb(255, 210, 120))
                            .strong(),
                    );
                    ui.label("Choose filters on the left, then click Load Chart or Run Backtest.");
                });
                return;
            };

            match self.selected_tab {
                GuiTab::Overview => self.render_overview(ui, &snapshot),
                GuiTab::Market => self.render_market(ui, &snapshot),
                GuiTab::Pnl => self.render_pnl(ui, &snapshot),
                GuiTab::Trades => render_trades(ui, &snapshot),
            }
        });
    }
}

fn render_metric_cards(ui: &mut Ui, snapshot: &DashboardSnapshot) {
    ui.heading("Recorder");
    Grid::new("metric_cards").num_columns(2).show(ui, |ui| {
        ui.label("DB");
        ui.monospace(snapshot.db_path.display().to_string());
        ui.end_row();
        ui.label("Book ticks");
        ui.strong(snapshot.recorder_metrics.book_ticker_events.to_string());
        ui.end_row();
        ui.label("Liquidations");
        ui.strong(snapshot.recorder_metrics.liquidation_events.to_string());
        ui.end_row();
        ui.label("Agg trades");
        ui.strong(snapshot.recorder_metrics.agg_trade_events.to_string());
        ui.end_row();
        ui.label("1s bars");
        ui.strong(snapshot.recorder_metrics.derived_kline_1s_bars.to_string());
        ui.end_row();
    });
}

fn render_trades(ui: &mut Ui, snapshot: &DashboardSnapshot) {
    let Some(report) = &snapshot.selected_report else {
        ui.label("No backtest report selected.");
        return;
    };
    egui::ScrollArea::vertical().show(ui, |ui| {
        Grid::new("trades_table")
            .striped(true)
            .num_columns(8)
            .show(ui, |ui| {
                ui.strong("Trade");
                ui.strong("Trigger");
                ui.strong("Entry");
                ui.strong("Exit");
                ui.strong("Reason");
                ui.strong("Qty");
                ui.strong("Net PnL");
                ui.strong("Price");
                ui.end_row();

                for trade in &report.trades {
                    ui.label(trade.trade_id.to_string());
                    ui.label(trade.trigger_time.format("%m-%d %H:%M:%S").to_string());
                    ui.label(trade.entry_time.format("%m-%d %H:%M:%S").to_string());
                    ui.label(
                        trade
                            .exit_time
                            .map(|value| value.format("%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    );
                    ui.label(
                        trade
                            .exit_reason
                            .as_ref()
                            .map(|value| value.as_str())
                            .unwrap_or("-"),
                    );
                    ui.label(format!("{:.4}", trade.qty));
                    ui.label(
                        trade
                            .net_pnl
                            .map(|value| format!("{value:.2}"))
                            .unwrap_or_else(|| "-".to_string()),
                    );
                    ui.label(format!(
                        "{:.2} -> {}",
                        trade.entry_price,
                        trade
                            .exit_price
                            .map(|value| format!("{value:.2}"))
                            .unwrap_or_else(|| "-".to_string())
                    ));
                    ui.end_row();
                }
            });
    });
}

fn parse_date(label: &str, value: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|error| format!("invalid {label} date: {error}"))
}

fn render_request(ui: &Ui, size: egui::Vec2) -> RenderRequest {
    RenderRequest {
        width_px: size.x.round().max(320.0) as u32,
        height_px: size.y.round().max(220.0) as u32,
        pixel_ratio: ui.ctx().pixels_per_point(),
        oversample: 1,
    }
}

struct ChartInteraction<'a> {
    response: egui::Response,
    texture: &'a mut RetainedChartTexture,
    viewport: &'a mut Viewport,
}

fn apply_hover(
    ui: &mut Ui,
    renderer: &PlottersRenderer,
    scene: &mut ChartScene,
    request: &RenderRequest,
    interaction: ChartInteraction<'_>,
) {
    let ChartInteraction {
        response,
        texture,
        viewport,
    } = interaction;
    let size = response.rect.size();
    let Some(pointer) = response.hover_pos() else {
        return;
    };
    let local_x = pointer.x - response.rect.left();
    let local_y = pointer.y - response.rect.top();
    let mut should_rerender = false;
    if response.dragged() {
        let delta_x = ui.input(|input| input.pointer.delta().x);
        if delta_x.abs() > f32::EPSILON {
            pan_scene(scene, -(delta_x / size.x.max(1.0)));
            should_rerender = true;
        }
    }
    if response.double_clicked() {
        scene.viewport = Viewport::default();
        should_rerender = true;
    }
    if response.hovered() {
        let scroll_delta = ui.input(|input| input.raw_scroll_delta.y);
        if scroll_delta.abs() > f32::EPSILON {
            zoom_scene(
                scene,
                (local_x / size.x.max(1.0)).clamp(0.0, 1.0),
                scroll_delta / 120.0,
            );
            should_rerender = true;
        }
    }
    let Some(hover) = hover_model_at(scene, size.x, size.y, local_x, local_y) else {
        *viewport = scene.viewport.clone();
        return;
    };
    if let Some(tooltip) = &hover.tooltip {
        let tooltip_x = if pointer.x > response.rect.center().x {
            (pointer.x - 320.0).max(response.rect.left() + 12.0)
        } else {
            (pointer.x + 20.0).min(response.rect.right() - 12.0)
        };
        let tooltip_y = if pointer.y > response.rect.center().y {
            (pointer.y - 24.0).max(response.rect.top() + 12.0)
        } else {
            (pointer.y + 20.0).min(response.rect.bottom() - 12.0)
        };
        egui::show_tooltip_at(
            ui.ctx(),
            ui.layer_id(),
            response.id.with("chart-tooltip"),
            egui::pos2(tooltip_x, tooltip_y),
            |ui| render_tooltip(ui, tooltip),
        );
    }
    draw_hover_overlay(ui, response.rect, pointer);
    scene.hover = Some(hover);
    *viewport = scene.viewport.clone();
    if should_rerender {
        if let Ok(frame) = renderer.render(scene, request) {
            texture.update(ui.ctx(), response.id.value().to_string().as_str(), &frame);
            ui.ctx().request_repaint();
        }
    }
}

fn draw_hover_overlay(ui: &mut Ui, rect: egui::Rect, pointer: egui::Pos2) {
    let stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(180, 190, 210, 180));
    let painter = ui.painter();
    painter.line_segment(
        [
            egui::pos2(pointer.x, rect.top() + 8.0),
            egui::pos2(pointer.x, rect.bottom() - 8.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(rect.left() + 8.0, pointer.y),
            egui::pos2(rect.right() - 8.0, pointer.y),
        ],
        stroke,
    );
}

fn render_tooltip(ui: &mut Ui, tooltip: &TooltipModel) {
    ui.strong(tooltip.title.as_str());
    ui.separator();
    for section in &tooltip.sections {
        ui.group(|ui| {
            ui.label(RichText::new(section.title.as_str()).strong());
            ui.add_space(4.0);
            egui::Grid::new(format!("tooltip-section-{}", section.title))
                .num_columns(2)
                .show(ui, |ui| {
                    for row in &section.rows {
                        ui.label(row.label.as_str());
                        ui.monospace(row.value.as_str());
                        ui.end_row();
                    }
                });
        });
        ui.add_space(6.0);
    }
}

fn render_chart_period_label(ui: &mut Ui, scene: &ChartScene, interval_label: &str) {
    if let Some((from, to)) = visible_time_bounds(scene) {
        ui.small(format!(
            "Period: {} → {} ({}) | Unit: {}",
            format_epoch_ms(from.as_i64()),
            format_epoch_ms(to.as_i64()),
            human_duration(to.as_i64().saturating_sub(from.as_i64())),
            interval_label,
        ));
        ui.add_space(4.0);
    }
}

fn format_epoch_ms(value: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(value)
        .map(|time| time.format("%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn human_duration(span_ms: i64) -> String {
    let total_seconds = (span_ms / 1_000).max(0);
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn market_period_unit_label(
    snapshot: &DashboardSnapshot,
    selected_timeframe: MarketTimeframe,
) -> String {
    if snapshot.market_series.klines.is_empty() {
        return "book-ticker".to_string();
    }
    source_interval_hint(snapshot, selected_timeframe)
        .map(|source| format!("{} (src {source})", selected_timeframe.label()))
        .unwrap_or_else(|| selected_timeframe.label().to_string())
}

fn source_interval_hint(
    snapshot: &DashboardSnapshot,
    selected_timeframe: MarketTimeframe,
) -> Option<String> {
    snapshot
        .market_series
        .kline_interval
        .as_deref()
        .and_then(MarketTimeframe::from_interval_label)
        .filter(|source| source.rank() > selected_timeframe.rank())
        .map(|source| source.label().to_string())
}

fn recommended_timeframes_from_source(source_interval: &str) -> Vec<MarketTimeframe> {
    let Some(source) = MarketTimeframe::from_interval_label(source_interval) else {
        return Vec::new();
    };
    MarketTimeframe::all()
        .into_iter()
        .filter(|timeframe| timeframe.rank() >= source.rank())
        .take(4)
        .collect()
}

fn latest_available_data_day(
    metrics: &crate::dataset::types::RecorderMetrics,
) -> Option<chrono::NaiveDate> {
    [
        metrics.last_book_ticker_event_time.as_deref(),
        metrics.last_agg_trade_event_time.as_deref(),
        metrics.last_liquidation_event_time.as_deref(),
    ]
    .into_iter()
    .flatten()
    .find_map(parse_metrics_date)
}

fn parse_metrics_date(value: &str) -> Option<chrono::NaiveDate> {
    value
        .get(0..10)
        .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

#[cfg(test)]
mod tests {
    use duckdb::Connection;

    use super::*;
    use crate::dataset::schema::init_schema_for_path;

    #[test]
    fn parse_metrics_date_extracts_leading_day() {
        let date = parse_metrics_date("2026-03-13 08:47:14.825").expect("date");
        assert_eq!(
            date,
            chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("valid date")
        );
    }

    #[test]
    fn latest_available_data_day_prefers_first_available_metric() {
        let metrics = crate::dataset::types::RecorderMetrics {
            last_book_ticker_event_time: Some("2026-03-13 08:47:14.825".to_string()),
            last_agg_trade_event_time: Some("2026-03-12 08:47:14.825".to_string()),
            ..Default::default()
        };

        assert_eq!(
            latest_available_data_day(&metrics),
            chrono::NaiveDate::from_ymd_opt(2026, 3, 13)
        );
    }

    #[test]
    fn market_period_unit_label_surfaces_coarser_source_interval() {
        let snapshot = DashboardSnapshot {
            mode: BinanceMode::Demo,
            base_dir: std::path::PathBuf::from("var"),
            db_path: std::path::PathBuf::from("var/demo.duckdb"),
            symbol: "BTCUSDT".to_string(),
            from: chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
            to: chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
            available_symbols: vec!["BTCUSDT".to_string()],
            recorder_metrics: crate::dataset::types::RecorderMetrics::default(),
            dataset_summary: crate::dataset::types::BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "BTCUSDT".to_string(),
                symbol_found: true,
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                liquidation_events: 0,
                book_ticker_events: 0,
                agg_trade_events: 0,
                derived_kline_1s_bars: 0,
            },
            market_series: crate::visualization::types::MarketSeries {
                symbol: "BTCUSDT".to_string(),
                liquidations: Vec::new(),
                book_tickers: Vec::new(),
                klines: vec![crate::dataset::types::DerivedKlineRow {
                    open_time_ms: 0,
                    close_time_ms: 1,
                    open: 1.0,
                    high: 1.0,
                    low: 1.0,
                    close: 1.0,
                    volume: 1.0,
                    quote_volume: 1.0,
                    trade_count: 1,
                }],
                kline_interval: Some("1d".to_string()),
            },
            recent_runs: Vec::new(),
            selected_report: None,
            selected_run_id: None,
        };

        assert_eq!(
            market_period_unit_label(&snapshot, MarketTimeframe::Minute1m),
            "1m (src 1d)"
        );
    }

    #[test]
    fn source_interval_hint_reports_coarser_source_without_mutating_selection() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("date");
        let snapshot = DashboardSnapshot {
            mode: BinanceMode::Demo,
            base_dir: std::path::PathBuf::from("var"),
            db_path: std::path::PathBuf::from("var/demo.duckdb"),
            symbol: "BTCUSDT".to_string(),
            from: today,
            to: today,
            available_symbols: vec!["BTCUSDT".to_string()],
            recorder_metrics: crate::dataset::types::RecorderMetrics::default(),
            dataset_summary: crate::dataset::types::BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "BTCUSDT".to_string(),
                symbol_found: true,
                from: today.to_string(),
                to: today.to_string(),
                liquidation_events: 0,
                book_ticker_events: 0,
                agg_trade_events: 0,
                derived_kline_1s_bars: 0,
            },
            market_series: crate::visualization::types::MarketSeries {
                symbol: "BTCUSDT".to_string(),
                liquidations: Vec::new(),
                book_tickers: Vec::new(),
                klines: vec![crate::dataset::types::DerivedKlineRow {
                    open_time_ms: 0,
                    close_time_ms: 1,
                    open: 1.0,
                    high: 1.0,
                    low: 1.0,
                    close: 1.0,
                    volume: 1.0,
                    quote_volume: 1.0,
                    trade_count: 1,
                }],
                kline_interval: Some("15m".to_string()),
            },
            recent_runs: Vec::new(),
            selected_report: None,
            selected_run_id: None,
        };

        assert_eq!(
            source_interval_hint(&snapshot, MarketTimeframe::Minute1m).as_deref(),
            Some("15m")
        );
    }

    #[test]
    fn recommended_timeframes_start_from_source_interval() {
        let labels = recommended_timeframes_from_source("15m")
            .into_iter()
            .map(|timeframe| timeframe.label())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["15m", "30m", "1h", "4h"]);
    }

    #[test]
    fn date_presets_update_input_ranges() {
        let today = chrono::Utc::now().date_naive();
        let mut app = SandboxQuantGuiApp::new(GuiLaunchConfig {
            mode: BinanceMode::Demo,
            base_dir: "var".to_string(),
            symbol: "BTCUSDT".to_string(),
            from: today,
            to: today,
            market_timeframe: MarketTimeframe::Tick1s,
        });

        app.apply_today_preset();
        assert_eq!(app.from_input, today.to_string());
        assert_eq!(app.to_input, today.to_string());

        app.apply_last_days_preset(2);
        assert_eq!(app.from_input, (today - chrono::Days::new(1)).to_string());
        assert_eq!(app.to_input, today.to_string());

        app.apply_last_days_preset(7);
        assert_eq!(app.from_input, (today - chrono::Days::new(6)).to_string());
        assert_eq!(app.to_input, today.to_string());
    }

    #[test]
    fn reset_viewports_clears_chart_ranges() {
        let today = chrono::Utc::now().date_naive();
        let mut app = SandboxQuantGuiApp::new(GuiLaunchConfig {
            mode: BinanceMode::Demo,
            base_dir: "var".to_string(),
            symbol: "BTCUSDT".to_string(),
            from: today,
            to: today,
            market_timeframe: MarketTimeframe::Tick1s,
        });
        app.market_viewport.x_range = Some((
            crate::charting::scene::EpochMs::new(1),
            crate::charting::scene::EpochMs::new(2),
        ));
        app.equity_viewport.x_range = Some((
            crate::charting::scene::EpochMs::new(3),
            crate::charting::scene::EpochMs::new(4),
        ));

        app.reset_viewports();

        assert!(app.market_viewport.x_range.is_none());
        assert!(app.equity_viewport.x_range.is_none());
    }

    #[test]
    fn refresh_dashboard_with_fallback_loads_latest_available_day_from_raw_klines() {
        let mut base_dir = std::env::temp_dir();
        base_dir.push(format!(
            "sandbox_quant_gui_fallback_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&base_dir).expect("create temp dir");
        let db_path = base_dir.join("market-v2-demo.duckdb");
        init_schema_for_path(&db_path).expect("init schema");
        let connection = Connection::open(&db_path).expect("open db");
        connection
            .execute(
                "INSERT INTO raw_klines (
                kline_id, mode, product, symbol, interval, open_time, close_time,
                open, high, low, close, volume, quote_volume, trade_count, raw_payload
             ) VALUES (
                1, 'demo', 'um', 'BTCUSDT', '1m',
                CAST('2026-03-13 00:00:00' AS TIMESTAMP),
                CAST('2026-03-13 00:00:59' AS TIMESTAMP),
                100.0, 101.0, 99.5, 100.5, 10.0, 1005.0, 5, '{}'
             )",
                [],
            )
            .expect("insert raw kline");

        let mut app = SandboxQuantGuiApp::new(GuiLaunchConfig {
            mode: BinanceMode::Demo,
            base_dir: base_dir.display().to_string(),
            symbol: "BTCUSDT".to_string(),
            from: chrono::NaiveDate::from_ymd_opt(2026, 3, 14).expect("date"),
            to: chrono::NaiveDate::from_ymd_opt(2026, 3, 14).expect("date"),
            market_timeframe: MarketTimeframe::Tick1s,
        });

        app.refresh_dashboard_with_fallback(None, true);

        assert_eq!(app.from_input, "2026-03-13");
        assert_eq!(app.to_input, "2026-03-13");
        assert!(app.snapshot.is_some());

        std::fs::remove_file(db_path).ok();
        std::fs::remove_dir_all(base_dir).ok();
    }

    #[test]
    fn run_backtest_button_path_produces_selected_report_and_pnl_tab() {
        let mut base_dir = std::env::temp_dir();
        base_dir.push(format!(
            "sandbox_quant_gui_backtest_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&base_dir).expect("create temp dir");
        let db_path = base_dir.join("market-v2-demo.duckdb");
        init_schema_for_path(&db_path).expect("init schema");

        let mut app = SandboxQuantGuiApp::new(GuiLaunchConfig {
            mode: BinanceMode::Demo,
            base_dir: base_dir.display().to_string(),
            symbol: "BTCUSDT".to_string(),
            from: chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
            to: chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
            market_timeframe: MarketTimeframe::Tick1s,
        });

        app.run_backtest();

        assert!(app
            .snapshot
            .as_ref()
            .and_then(|s| s.selected_report.as_ref())
            .is_some());
        assert_eq!(app.selected_tab, GuiTab::Pnl);

        std::fs::remove_file(db_path).ok();
        std::fs::remove_dir_all(base_dir).ok();
    }
}
