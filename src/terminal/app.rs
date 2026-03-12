use crate::terminal::completion::ShellCompletion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMode {
    Raw,
    Line,
}

pub enum TerminalEvent {
    NoOutput,
    Output(String),
    Exit,
}

pub trait TerminalApp {
    fn terminal_mode(&self) -> TerminalMode {
        TerminalMode::Raw
    }
    fn intro_panel(&self) -> String;
    fn help_heading(&self) -> &'static str {
        "slash commands"
    }
    fn help_text(&self) -> String;
    fn prompt(&self) -> String;
    fn complete(&self, line: &str) -> Vec<ShellCompletion>;
    fn execute_line(&mut self, line: &str) -> Result<TerminalEvent, String>;
}
