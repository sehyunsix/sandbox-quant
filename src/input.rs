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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridCommand {
    TabAssets,
    TabStrategies,
    TabRisk,
    TabNetwork,
    TabSystemLog,
    ToggleOnOffPanel,
    StrategyUp,
    StrategyDown,
    SymbolLeft,
    SymbolRight,
    NewStrategy,
    EditStrategyConfig,
    DeleteStrategy,
    ToggleStrategyOnOff,
    ActivateStrategy,
    CloseGrid,
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

pub fn parse_grid_command(key_code: &KeyCode) -> Option<GridCommand> {
    match key_code {
        KeyCode::Char('1') => Some(GridCommand::TabAssets),
        KeyCode::Char('2') => Some(GridCommand::TabStrategies),
        KeyCode::Char('3') => Some(GridCommand::TabRisk),
        KeyCode::Char('4') => Some(GridCommand::TabNetwork),
        KeyCode::Char('5') => Some(GridCommand::TabSystemLog),
        KeyCode::Tab => Some(GridCommand::ToggleOnOffPanel),
        KeyCode::Up => Some(GridCommand::StrategyUp),
        KeyCode::Down => Some(GridCommand::StrategyDown),
        KeyCode::Left => Some(GridCommand::SymbolLeft),
        KeyCode::Right => Some(GridCommand::SymbolRight),
        KeyCode::Delete => Some(GridCommand::DeleteStrategy),
        KeyCode::Enter => Some(GridCommand::ActivateStrategy),
        KeyCode::Esc => Some(GridCommand::CloseGrid),
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'k' => Some(GridCommand::StrategyUp),
            'j' => Some(GridCommand::StrategyDown),
            'h' => Some(GridCommand::SymbolLeft),
            'l' => Some(GridCommand::SymbolRight),
            'n' => Some(GridCommand::NewStrategy),
            'c' => Some(GridCommand::EditStrategyConfig),
            'x' => Some(GridCommand::DeleteStrategy),
            'o' => Some(GridCommand::ToggleStrategyOnOff),
            'f' => Some(GridCommand::ActivateStrategy),
            'g' => Some(GridCommand::CloseGrid),
            _ => None,
        },
        _ => None,
    }
}
