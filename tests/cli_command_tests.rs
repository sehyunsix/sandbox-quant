use sandbox_quant::app::cli::{
    complete_shell_input, parse_app_command, parse_shell_input, shell_help_text, ShellInput,
};
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::shell::{
    format_completion_line, next_completion_index, previous_completion_index,
};
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::execution::command::{CommandSource, ExecutionCommand};

#[test]
fn parse_refresh_command_by_default() {
    let command = parse_app_command(&[]).expect("default command should parse");
    assert_eq!(command, AppCommand::RefreshAuthoritativeState);
}

#[test]
fn parse_close_all_command() {
    let command = parse_app_command(&["close-all".to_string()]).expect("close-all should parse");
    assert_eq!(
        command,
        AppCommand::Execution(ExecutionCommand::CloseAll {
            source: CommandSource::User,
        })
    );
}

#[test]
fn parse_close_symbol_command() {
    let command = parse_app_command(&[
        "close-symbol".to_string(),
        "BTCUSDT".to_string(),
    ])
    .expect("close-symbol should parse");

    assert_eq!(
        command,
        AppCommand::Execution(ExecutionCommand::CloseSymbol {
            instrument: Instrument::new("BTCUSDT"),
            source: CommandSource::User,
        })
    );
}

#[test]
fn parse_set_target_exposure_command() {
    let command = parse_app_command(&[
        "set-target-exposure".to_string(),
        "ETHUSDT".to_string(),
        "0.25".to_string(),
    ])
    .expect("set-target-exposure should parse");

    match command {
        AppCommand::Execution(ExecutionCommand::SetTargetExposure {
            instrument,
            target,
            source,
        }) => {
            assert_eq!(instrument, Instrument::new("ETHUSDT"));
            assert_eq!(target.value(), 0.25);
            assert_eq!(source, CommandSource::User);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parse_rejects_out_of_range_target_exposure() {
    let error = parse_app_command(&[
        "set-target-exposure".to_string(),
        "ETHUSDT".to_string(),
        "2.0".to_string(),
    ])
    .expect_err("out of range exposure should fail");

    assert!(error.contains("out of range"));
}

#[test]
fn parse_shell_input_supports_slash_commands() {
    let parsed = parse_shell_input("/close-all").expect("slash command should parse");

    assert_eq!(
        parsed,
        ShellInput::Command(AppCommand::Execution(ExecutionCommand::CloseAll {
            source: CommandSource::User,
        }))
    );
}

#[test]
fn parse_shell_input_supports_help_and_exit() {
    assert_eq!(
        parse_shell_input("/help").expect("help should parse"),
        ShellInput::Help
    );
    assert_eq!(
        parse_shell_input("/exit").expect("exit should parse"),
        ShellInput::Exit
    );
    assert!(shell_help_text().contains("/refresh"));
}

#[test]
fn parse_shell_input_supports_mode_switch() {
    assert_eq!(
        parse_shell_input("/mode demo").expect("mode should parse"),
        ShellInput::Mode(BinanceMode::Demo)
    );
}

#[test]
fn shell_completion_suggests_commands_modes_and_instruments() {
    let command_matches = complete_shell_input("/cl", &[]);
    assert!(command_matches.contains(&"/close-all".to_string()));
    assert!(command_matches.contains(&"/close-symbol".to_string()));

    let mode_matches = complete_shell_input("/mode d", &[]);
    assert_eq!(mode_matches, vec!["/mode demo".to_string()]);

    let instrument_matches = complete_shell_input(
        "/close-symbol BT",
        &["BTCUSDT".to_string(), "ETHUSDT".to_string()],
    );
    assert_eq!(instrument_matches, vec!["/close-symbol BTCUSDT".to_string()]);
}

#[test]
fn shell_completion_line_marks_selected_item() {
    let line = format_completion_line(
        &["/refresh".to_string(), "/close-all".to_string()],
        1,
    );

    assert_eq!(line, "/refresh  [/close-all]");
}

#[test]
fn completion_index_wraps_for_up_and_down_navigation() {
    assert_eq!(next_completion_index(3, 0), 1);
    assert_eq!(next_completion_index(3, 2), 0);
    assert_eq!(previous_completion_index(3, 0), 2);
    assert_eq!(previous_completion_index(3, 2), 1);
}
