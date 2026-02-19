use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    Pause,
    Resume,
    ManualBuy,
    ManualSell,
    SwitchTimeframe(&'static str),
    OpenSymbolSelector,
    OpenStrategySelector,
    OpenAccountPopup,
    OpenHistoryPopup,
    OpenGrid,
}

pub fn parse_main_command(key_code: &KeyCode) -> Option<UiCommand> {
    match key_code {
        KeyCode::Char('0') => Some(UiCommand::SwitchTimeframe("1s")),
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'p' => Some(UiCommand::Pause),
            'r' => Some(UiCommand::Resume),
            'b' => Some(UiCommand::ManualBuy),
            's' => Some(UiCommand::ManualSell),
            '1' => Some(UiCommand::SwitchTimeframe("1m")),
            'h' => Some(UiCommand::SwitchTimeframe("1h")),
            'd' => Some(UiCommand::SwitchTimeframe("1d")),
            'w' => Some(UiCommand::SwitchTimeframe("1w")),
            'm' => Some(UiCommand::SwitchTimeframe("1M")),
            't' => Some(UiCommand::OpenSymbolSelector),
            'y' => Some(UiCommand::OpenStrategySelector),
            'a' => Some(UiCommand::OpenAccountPopup),
            'i' => Some(UiCommand::OpenHistoryPopup),
            'g' | 'f' => Some(UiCommand::OpenGrid),
            _ => None,
        },
        _ => None,
    }
}
