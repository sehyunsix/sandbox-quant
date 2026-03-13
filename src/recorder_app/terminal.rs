use crate::app::bootstrap::BinanceMode;
use crate::command::recorder::{
    complete_recorder_input, parse_recorder_shell_input, recorder_help_text, RecorderCommand,
    RecorderShellInput,
};
use crate::record::coordination::RecorderCoordination;
use crate::recorder_app::runtime::{MarketDataRecorder, RecorderState};
use crate::terminal::app::{TerminalApp, TerminalEvent, TerminalMode};
use crate::terminal::completion::ShellCompletion;
use crate::ui::recorder_output::render_live_recorder_status;

pub struct RecorderTerminal {
    pub mode: BinanceMode,
    pub base_dir: String,
    pub recorder: MarketDataRecorder,
    pub coordination: RecorderCoordination,
    pub manual_symbols: Vec<String>,
}

impl RecorderTerminal {
    pub fn new(mode: BinanceMode, base_dir: impl Into<String>) -> Self {
        let base_dir = base_dir.into();
        Self {
            mode,
            recorder: MarketDataRecorder::new(base_dir.clone()),
            coordination: RecorderCoordination::new(base_dir.clone()),
            base_dir,
            manual_symbols: Vec::new(),
        }
    }

    fn sync_strategy_symbols(&mut self) -> Result<Vec<String>, String> {
        let strategy_symbols = self
            .coordination
            .strategy_symbols(self.mode)
            .map_err(|error| error.to_string())?;
        if self.recorder.status(self.mode).state == RecorderState::Running {
            self.recorder
                .update_strategy_symbols(self.mode, strategy_symbols.clone())
                .map_err(|error| error.to_string())?;
        }
        Ok(strategy_symbols)
    }
}

impl TerminalApp for RecorderTerminal {
    fn terminal_mode(&self) -> TerminalMode {
        TerminalMode::Line
    }

    fn intro_panel(&self) -> String {
        format!(
            "╭──────────────────────────────────────────────╮\n│ >_ Sandbox Quant Recorder (v{})              │\n│                                              │\n│ mode:      {:<18} /mode to change │\n│ base_dir:  {:<28} │\n╰──────────────────────────────────────────────╯",
            env!("CARGO_PKG_VERSION"),
            self.mode.as_str(),
            self.base_dir
        )
    }

    fn help_text(&self) -> String {
        recorder_help_text().to_string()
    }

    fn prompt(&self) -> String {
        format!("[recorder:{}] › ", self.mode.as_str())
    }

    fn complete(&self, line: &str) -> Vec<ShellCompletion> {
        complete_recorder_input(line)
    }

    fn execute_line(&mut self, line: &str) -> Result<TerminalEvent, String> {
        match parse_recorder_shell_input(line) {
            Ok(RecorderShellInput::Empty) => Ok(TerminalEvent::NoOutput),
            Ok(RecorderShellInput::Help) => Ok(TerminalEvent::Output(self.help_text())),
            Ok(RecorderShellInput::Exit) => Ok(TerminalEvent::Exit),
            Ok(RecorderShellInput::Mode(mode)) => {
                self.mode = mode;
                Ok(TerminalEvent::Output(format!(
                    "mode switched to {}",
                    self.mode.as_str()
                )))
            }
            Ok(RecorderShellInput::Command(command)) => match command {
                RecorderCommand::Start { symbols } => {
                    self.manual_symbols = symbols;
                    let strategy_symbols = self.sync_strategy_symbols()?;
                    let status = if self.recorder.status(self.mode).state == RecorderState::Running
                    {
                        self.recorder
                            .update_manual_symbols(self.mode, self.manual_symbols.clone())
                            .map_err(|error| error.to_string())?;
                        self.recorder.status(self.mode)
                    } else {
                        self.recorder
                            .start(self.mode, self.manual_symbols.clone(), strategy_symbols)
                            .map_err(|error| error.to_string())?
                    };
                    Ok(TerminalEvent::Output(render_live_recorder_status(
                        "record started",
                        &status,
                    )))
                }
                RecorderCommand::Status => {
                    let _ = self.sync_strategy_symbols()?;
                    let status = self.recorder.status(self.mode);
                    Ok(TerminalEvent::Output(render_live_recorder_status(
                        "record status",
                        &status,
                    )))
                }
                RecorderCommand::Stop => {
                    let status = self
                        .recorder
                        .stop(self.mode)
                        .map_err(|error| error.to_string())?;
                    Ok(TerminalEvent::Output(render_live_recorder_status(
                        "record stopped",
                        &status,
                    )))
                }
            },
            Err(error) => Err(error),
        }
    }
}
