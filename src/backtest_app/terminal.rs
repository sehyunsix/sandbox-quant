use crate::app::bootstrap::BinanceMode;
use crate::command::backtest::{
    backtest_help_text, complete_backtest_input, parse_backtest_shell_input, BacktestCommand,
    BacktestShellInput,
};
use crate::dataset::query::backtest_summary_for_path;
use crate::record::manager::format_mode;
use crate::terminal::app::{TerminalApp, TerminalEvent, TerminalMode};
use crate::terminal::completion::ShellCompletion;
use crate::ui::backtest_output::render_backtest_run;

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
            format_mode(self.mode),
            self.base_dir
        )
    }

    fn help_text(&self) -> String {
        backtest_help_text().to_string()
    }

    fn prompt(&self) -> String {
        format!("[backtest:{}] › ", format_mode(self.mode))
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
                    format_mode(self.mode)
                )))
            }
            Ok(BacktestShellInput::Command(command)) => match command {
                BacktestCommand::Run {
                    template,
                    instrument,
                    from,
                    to,
                } => {
                    let db_path = std::path::Path::new(&self.base_dir)
                        .join(format!("market-{}.duckdb", format_mode(self.mode)));
                    let summary =
                        backtest_summary_for_path(&db_path, self.mode, &instrument, from, to)
                            .map_err(|error| error.to_string())?;
                    Ok(TerminalEvent::Output(render_backtest_run(
                        template,
                        &instrument,
                        self.mode,
                        &db_path,
                        &summary,
                    )))
                }
            },
            Err(error) => Err(error),
        }
    }
}
