use crossterm::event::KeyCode;
use sandbox_quant::input::{parse_main_command, UiCommand};

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
