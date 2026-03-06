use crate::execution::command::ExecutionCommand;

#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    Execution(ExecutionCommand),
    RefreshAuthoritativeState,
}
