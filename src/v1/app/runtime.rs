use crate::v1::app::commands::AppCommand;

#[derive(Debug, Default)]
pub struct AppRuntime {
    pub last_command: Option<AppCommand>,
}

impl AppRuntime {
    pub fn record_command(&mut self, command: AppCommand) {
        self.last_command = Some(command);
    }
}
