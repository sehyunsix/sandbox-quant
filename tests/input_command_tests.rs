use crossterm::event::KeyCode;
use sandbox_quant::input::{
    parse_grid_command, parse_main_command, parse_popup_command, GridCommand, PopupCommand,
    PopupKind, UiCommand,
};

#[test]
fn parse_main_command_maps_case_insensitive_char_keys() {
    assert_eq!(parse_main_command(&KeyCode::Char('p')), Some(UiCommand::Pause));
    assert_eq!(parse_main_command(&KeyCode::Char('P')), Some(UiCommand::Pause));
    assert_eq!(parse_main_command(&KeyCode::Char('r')), Some(UiCommand::Resume));
    assert_eq!(
        parse_main_command(&KeyCode::Char('B')),
        Some(UiCommand::ManualBuy)
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('s')),
        Some(UiCommand::ManualSell)
    );
}

#[test]
fn parse_main_command_maps_timeframe_keys() {
    assert_eq!(
        parse_main_command(&KeyCode::Char('0')),
        Some(UiCommand::SwitchTimeframe("1s"))
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('1')),
        Some(UiCommand::SwitchTimeframe("1m"))
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('H')),
        Some(UiCommand::SwitchTimeframe("1h"))
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('d')),
        Some(UiCommand::SwitchTimeframe("1d"))
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('W')),
        Some(UiCommand::SwitchTimeframe("1w"))
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('m')),
        Some(UiCommand::SwitchTimeframe("1M"))
    );
}

#[test]
fn parse_main_command_maps_popup_and_grid_keys() {
    assert_eq!(
        parse_main_command(&KeyCode::Char('t')),
        Some(UiCommand::OpenSymbolSelector)
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('Y')),
        Some(UiCommand::OpenStrategySelector)
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('a')),
        Some(UiCommand::OpenAccountPopup)
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('i')),
        Some(UiCommand::OpenHistoryPopup)
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('g')),
        Some(UiCommand::OpenGrid)
    );
    assert_eq!(
        parse_main_command(&KeyCode::Char('F')),
        Some(UiCommand::OpenGrid)
    );
    assert_eq!(parse_main_command(&KeyCode::Esc), None);
}

#[test]
fn parse_grid_command_maps_navigation_and_actions() {
    assert_eq!(
        parse_grid_command(&KeyCode::Char('1')),
        Some(GridCommand::TabAssets)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('2')),
        Some(GridCommand::TabStrategies)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('3')),
        Some(GridCommand::TabRisk)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('4')),
        Some(GridCommand::TabNetwork)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('5')),
        Some(GridCommand::TabHistory)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('6')),
        Some(GridCommand::TabSystemLog)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Tab),
        Some(GridCommand::ToggleOnOffPanel)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('K')),
        Some(GridCommand::StrategyUp)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('j')),
        Some(GridCommand::StrategyDown)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('h')),
        Some(GridCommand::SymbolLeft)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('L')),
        Some(GridCommand::SymbolRight)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('n')),
        Some(GridCommand::NewStrategy)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('c')),
        Some(GridCommand::EditStrategyConfig)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Delete),
        Some(GridCommand::DeleteStrategy)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('o')),
        Some(GridCommand::ToggleStrategyOnOff)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Char('f')),
        Some(GridCommand::ActivateStrategy)
    );
    assert_eq!(
        parse_grid_command(&KeyCode::Esc),
        Some(GridCommand::CloseGrid)
    );
}

#[test]
fn parse_popup_command_maps_symbol_strategy_selectors() {
    assert_eq!(
        parse_popup_command(PopupKind::SymbolSelector, &KeyCode::Char('t')),
        Some(PopupCommand::Close)
    );
    assert_eq!(
        parse_popup_command(PopupKind::SymbolSelector, &KeyCode::Char('k')),
        Some(PopupCommand::Up)
    );
    assert_eq!(
        parse_popup_command(PopupKind::SymbolSelector, &KeyCode::Down),
        Some(PopupCommand::Down)
    );
    assert_eq!(
        parse_popup_command(PopupKind::SymbolSelector, &KeyCode::Enter),
        Some(PopupCommand::Confirm)
    );
    assert_eq!(
        parse_popup_command(PopupKind::StrategySelector, &KeyCode::Char('Y')),
        Some(PopupCommand::Close)
    );
    assert_eq!(
        parse_popup_command(PopupKind::StrategySelector, &KeyCode::Up),
        Some(PopupCommand::Up)
    );
}

#[test]
fn parse_popup_command_maps_account_history_focus() {
    assert_eq!(
        parse_popup_command(PopupKind::Account, &KeyCode::Enter),
        Some(PopupCommand::Close)
    );
    assert_eq!(
        parse_popup_command(PopupKind::History, &KeyCode::Char('d')),
        Some(PopupCommand::HistoryDay)
    );
    assert_eq!(
        parse_popup_command(PopupKind::History, &KeyCode::Char('H')),
        Some(PopupCommand::HistoryHour)
    );
    assert_eq!(
        parse_popup_command(PopupKind::History, &KeyCode::Char('m')),
        Some(PopupCommand::HistoryMonth)
    );
    assert_eq!(
        parse_popup_command(PopupKind::Focus, &KeyCode::Esc),
        Some(PopupCommand::Close)
    );
    assert_eq!(
        parse_popup_command(PopupKind::Focus, &KeyCode::Char('z')),
        None
    );
}
