use crate::app::bootstrap::BinanceMode;
use crate::backtest_app::runner::{run_backtest_for_path, BacktestConfig};
use crate::backtest_app::snapshot::maybe_prepare_snapshot_from_postgres;
use crate::command::backtest::{
    backtest_help_text, complete_backtest_input, parse_backtest_shell_input, BacktestCommand,
    BacktestShellInput,
};
use crate::dataset::query::{
    load_backtest_report, load_backtest_run_summaries, persist_backtest_report,
};
use crate::dataset::schema::init_schema_for_path;
use crate::record::coordination::RecorderCoordination;
use crate::terminal::app::{TerminalApp, TerminalEvent, TerminalMode};
use crate::terminal::completion::ShellCompletion;
use crate::ui::backtest_output::{render_backtest_run, render_backtest_run_list};

pub struct BacktestTerminal {
    pub mode: BinanceMode,
    pub base_dir: String,
}

impl BacktestTerminal {
    pub fn new(mode: BinanceMode, base_dir: impl Into<String>) -> Self {
        Self {
            mode,
            base_dir: base_dir.into(),
        }
    }
}

impl TerminalApp for BacktestTerminal {
    fn terminal_mode(&self) -> TerminalMode {
        TerminalMode::Line
    }

    fn intro_panel(&self) -> String {
        format!(
            "╭──────────────────────────────────────────────╮\n│ >_ Sandbox Quant Backtest (v{})              │\n│                                              │\n│ mode:      {:<18} /mode to change │\n│ base_dir:  {:<28} │\n╰──────────────────────────────────────────────╯",
            env!("CARGO_PKG_VERSION"),
            self.mode.as_str(),
            self.base_dir
        )
    }

    fn help_text(&self) -> String {
        backtest_help_text().to_string()
    }

    fn prompt(&self) -> String {
        format!("[backtest:{}] › ", self.mode.as_str())
    }

    fn complete(&self, line: &str) -> Vec<ShellCompletion> {
        complete_backtest_input(line)
    }

    fn execute_line(&mut self, line: &str) -> Result<TerminalEvent, String> {
        match parse_backtest_shell_input(line) {
            Ok(BacktestShellInput::Empty) => Ok(TerminalEvent::NoOutput),
            Ok(BacktestShellInput::Help) => Ok(TerminalEvent::Output(self.help_text())),
            Ok(BacktestShellInput::Exit) => Ok(TerminalEvent::Exit),
            Ok(BacktestShellInput::Mode(mode)) => {
                self.mode = mode;
                Ok(TerminalEvent::Output(format!(
                    "mode switched to {}",
                    self.mode.as_str()
                )))
            }
            Ok(BacktestShellInput::Command(command)) => match command {
                BacktestCommand::Run {
                    template,
                    instrument,
                    from,
                    to,
                } => {
                    let db_path =
                        RecorderCoordination::new(self.base_dir.clone()).db_path(self.mode);
                    if let Some(message) = maybe_prepare_snapshot_from_postgres(
                        self.mode,
                        &self.base_dir,
                        &instrument,
                        from,
                        to,
                    )
                    .map_err(|error| error.to_string())?
                    {
                        eprintln!("{message}");
                    }
                    init_schema_for_path(&db_path).map_err(|error| error.to_string())?;
                    let report = run_backtest_for_path(
                        &db_path,
                        self.mode,
                        template,
                        &instrument,
                        from,
                        to,
                        BacktestConfig::default(),
                    )
                    .map_err(|error| error.to_string())?;
                    let run_id = persist_backtest_report(&db_path, &report)
                        .map_err(|error| error.to_string())?;
                    let mut report = report;
                    report.run_id = Some(run_id);
                    Ok(TerminalEvent::Output(render_backtest_run(&report)))
                }
                BacktestCommand::List => {
                    let db_path =
                        RecorderCoordination::new(self.base_dir.clone()).db_path(self.mode);
                    let runs = load_backtest_run_summaries(&db_path, 20)
                        .map_err(|error| error.to_string())?;
                    Ok(TerminalEvent::Output(render_backtest_run_list(&runs)))
                }
                BacktestCommand::ReportLatest => {
                    let db_path =
                        RecorderCoordination::new(self.base_dir.clone()).db_path(self.mode);
                    let report =
                        load_backtest_report(&db_path, None).map_err(|error| error.to_string())?;
                    if let Some(report) = report {
                        Ok(TerminalEvent::Output(render_backtest_run(&report)))
                    } else {
                        Ok(TerminalEvent::Output(
                            "backtest report\nstate=missing".to_string(),
                        ))
                    }
                }
                BacktestCommand::ReportShow { run_id } => {
                    let db_path =
                        RecorderCoordination::new(self.base_dir.clone()).db_path(self.mode);
                    let report = load_backtest_report(&db_path, Some(run_id))
                        .map_err(|error| error.to_string())?;
                    if let Some(report) = report {
                        Ok(TerminalEvent::Output(render_backtest_run(&report)))
                    } else {
                        Ok(TerminalEvent::Output(format!(
                            "backtest report\nrun_id={run_id}\nstate=missing"
                        )))
                    }
                }
            },
            Err(error) => Err(error),
        }
    }
}
