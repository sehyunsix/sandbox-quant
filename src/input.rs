use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    Pause,
    Resume,
    ManualBuy,
    ManualSell,
    CloseAllPositions,
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
    TabHistory,
    TabPositions,
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
    ToggleSmallPositionsFilter,
    CloseGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    SymbolSelector,
    StrategySelector,
    Account,
    History,
    Focus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupCommand {
    Close,
    Up,
    Down,
    Confirm,
    HistoryDay,
    HistoryHour,
    HistoryMonth,
}

pub fn parse_main_command(key_code: &KeyCode) -> Option<UiCommand> {
    match key_code {
        KeyCode::Char('0') => Some(UiCommand::SwitchTimeframe("1s")),
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'p' => Some(UiCommand::Pause),
            'r' => Some(UiCommand::Resume),
            'b' => Some(UiCommand::ManualBuy),
            's' => Some(UiCommand::ManualSell),
            'z' => Some(UiCommand::CloseAllPositions),
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
        KeyCode::Char('3') => Some(GridCommand::TabPositions),
        KeyCode::Char('4') => Some(GridCommand::TabRisk),
        KeyCode::Char('5') => Some(GridCommand::TabNetwork),
        KeyCode::Char('6') => Some(GridCommand::TabHistory),
        KeyCode::Char('7') => Some(GridCommand::TabSystemLog),
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
            'u' => Some(GridCommand::ToggleSmallPositionsFilter),
            'g' => Some(GridCommand::CloseGrid),
            _ => None,
        },
        _ => None,
    }
}

pub fn parse_popup_command(kind: PopupKind, key_code: &KeyCode) -> Option<PopupCommand> {
    match kind {
        PopupKind::SymbolSelector => match key_code {
            KeyCode::Esc | KeyCode::Char('t') | KeyCode::Char('T') => Some(PopupCommand::Close),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Some(PopupCommand::Up),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Some(PopupCommand::Down),
            KeyCode::Enter => Some(PopupCommand::Confirm),
            _ => None,
        },
        PopupKind::StrategySelector => match key_code {
            KeyCode::Esc | KeyCode::Char('y') | KeyCode::Char('Y') => Some(PopupCommand::Close),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Some(PopupCommand::Up),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Some(PopupCommand::Down),
            KeyCode::Enter => Some(PopupCommand::Confirm),
            _ => None,
        },
        PopupKind::Account => match key_code {
            KeyCode::Esc | KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Enter => {
                Some(PopupCommand::Close)
            }
            _ => None,
        },
        PopupKind::History => match key_code {
            KeyCode::Char('d') | KeyCode::Char('D') => Some(PopupCommand::HistoryDay),
            KeyCode::Char('h') | KeyCode::Char('H') => Some(PopupCommand::HistoryHour),
            KeyCode::Char('m') | KeyCode::Char('M') => Some(PopupCommand::HistoryMonth),
            KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Enter => {
                Some(PopupCommand::Close)
            }
            _ => None,
        },
        PopupKind::Focus => match key_code {
            KeyCode::Esc | KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Enter => {
                Some(PopupCommand::Close)
            }
            _ => None,
        },
    }
}
