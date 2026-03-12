use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::cli::{
    complete_shell_input, complete_shell_input_with_description,
    complete_shell_input_with_market_data, normalize_instrument_symbol, parse_app_command,
    parse_shell_input, shell_help_text, ShellInput,
};
use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::commands::PortfolioView;
use sandbox_quant::app::shell::{
    format_completion_line, next_completion_index, previous_completion_index, scroll_lines_needed,
};
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::order_type::OrderType;
use sandbox_quant::domain::position::Side;
use sandbox_quant::execution::command::{CommandSource, ExecutionCommand};
use sandbox_quant::strategy::command::{StrategyCommand, StrategyStartConfig};
use sandbox_quant::strategy::model::StrategyTemplate;
use sandbox_quant::ui::operator_terminal::shell_intro_panel;

#[test]
fn parse_refresh_command_by_default() {
    let command = parse_app_command(&[]).expect("default command should parse");
    assert_eq!(command, AppCommand::Portfolio(PortfolioView::Overview));
}

#[test]
fn parse_portfolio_subcommands_and_aliases() {
    assert_eq!(
        parse_app_command(&["portfolio".to_string(), "positions".to_string()])
            .expect("portfolio positions should parse"),
        AppCommand::Portfolio(PortfolioView::Positions)
    );
    assert_eq!(
        parse_app_command(&["balances".to_string()]).expect("balances alias should parse"),
        AppCommand::Portfolio(PortfolioView::Balances)
    );
    assert_eq!(
        parse_app_command(&["orders".to_string()]).expect("orders alias should parse"),
        AppCommand::Portfolio(PortfolioView::Orders)
    );
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
    let command = parse_app_command(&["close-symbol".to_string(), "BTCUSDT".to_string()])
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
fn parse_close_symbol_normalizes_base_symbol() {
    let command = parse_app_command(&["close-symbol".to_string(), "btc".to_string()])
        .expect("close-symbol should normalize base symbol");

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
            order_type,
            source,
        }) => {
            assert_eq!(instrument, Instrument::new("ETHUSDT"));
            assert_eq!(target.value(), 0.25);
            assert_eq!(order_type, OrderType::Market);
            assert_eq!(source, CommandSource::User);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parse_set_target_exposure_normalizes_base_symbol() {
    let command = parse_app_command(&[
        "set-target-exposure".to_string(),
        "eth".to_string(),
        "0.25".to_string(),
    ])
    .expect("set-target-exposure should normalize base symbol");

    match command {
        AppCommand::Execution(ExecutionCommand::SetTargetExposure {
            instrument,
            target,
            order_type,
            source,
        }) => {
            assert_eq!(instrument, Instrument::new("ETHUSDT"));
            assert_eq!(target.value(), 0.25);
            assert_eq!(order_type, OrderType::Market);
            assert_eq!(source, CommandSource::User);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parse_set_target_exposure_limit_command() {
    let command = parse_app_command(&[
        "set-target-exposure".to_string(),
        "ETHUSDT".to_string(),
        "0.25".to_string(),
        "limit".to_string(),
        "2600".to_string(),
    ])
    .expect("set-target-exposure limit should parse");

    match command {
        AppCommand::Execution(ExecutionCommand::SetTargetExposure {
            instrument,
            target,
            order_type,
            source,
        }) => {
            assert_eq!(instrument, Instrument::new("ETHUSDT"));
            assert_eq!(target.value(), 0.25);
            assert_eq!(order_type, OrderType::Limit { price: 2600.0 });
            assert_eq!(source, CommandSource::User);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parse_option_order_command() {
    let command = parse_app_command(&[
        "option-order".to_string(),
        "btc-260327-200000-c".to_string(),
        "buy".to_string(),
        "0.01".to_string(),
        "5".to_string(),
    ])
    .expect("option-order should parse");

    match command {
        AppCommand::Execution(ExecutionCommand::SubmitOptionOrder {
            instrument,
            side,
            qty,
            order_type,
            source,
        }) => {
            assert_eq!(instrument, Instrument::new("BTC-260327-200000-C"));
            assert_eq!(side, Side::Buy);
            assert_eq!(qty, 0.01);
            assert_eq!(order_type, OrderType::Limit { price: 5.0 });
            assert_eq!(source, CommandSource::User);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parse_strategy_start_command() {
    let command = parse_app_command(&[
        "strategy".to_string(),
        "start".to_string(),
        "liquidation-breakdown-short".to_string(),
        "btc".to_string(),
        "--risk-pct".to_string(),
        "0.005".to_string(),
        "--win-rate".to_string(),
        "0.8".to_string(),
        "--r".to_string(),
        "1.5".to_string(),
        "--max-entry-slippage".to_string(),
        "0.001".to_string(),
    ])
    .expect("strategy start should parse");

    assert_eq!(
        command,
        AppCommand::Strategy(StrategyCommand::Start {
            template: StrategyTemplate::LiquidationBreakdownShort,
            instrument: Instrument::new("BTCUSDT"),
            config: StrategyStartConfig {
                risk_pct: 0.005,
                win_rate: 0.8,
                r_multiple: 1.5,
                max_entry_slippage_pct: 0.001,
            },
        })
    );
}

#[test]
fn parse_strategy_start_command_uses_defaults_when_flags_are_omitted() {
    let command = parse_app_command(&[
        "strategy".to_string(),
        "start".to_string(),
        "liquidation-breakdown-short".to_string(),
        "btc".to_string(),
    ])
    .expect("strategy start should parse with defaults");

    assert_eq!(
        command,
        AppCommand::Strategy(StrategyCommand::Start {
            template: StrategyTemplate::LiquidationBreakdownShort,
            instrument: Instrument::new("BTCUSDT"),
            config: StrategyStartConfig {
                risk_pct: 0.005,
                win_rate: 0.8,
                r_multiple: 1.5,
                max_entry_slippage_pct: 0.001,
            },
        })
    );
}

#[test]
fn parse_strategy_surface_commands() {
    assert_eq!(
        parse_app_command(&["strategy".to_string(), "templates".to_string()])
            .expect("templates should parse"),
        AppCommand::Strategy(StrategyCommand::Templates)
    );
    assert_eq!(
        parse_app_command(&["strategy".to_string(), "list".to_string()])
            .expect("list should parse"),
        AppCommand::Strategy(StrategyCommand::List)
    );
    assert_eq!(
        parse_app_command(&["strategy".to_string(), "show".to_string(), "7".to_string()])
            .expect("show should parse"),
        AppCommand::Strategy(StrategyCommand::Show { watch_id: 7 })
    );
    assert_eq!(
        parse_app_command(&["strategy".to_string(), "stop".to_string(), "7".to_string()])
            .expect("stop should parse"),
        AppCommand::Strategy(StrategyCommand::Stop { watch_id: 7 })
    );
    assert_eq!(
        parse_app_command(&["strategy".to_string(), "history".to_string()])
            .expect("history should parse"),
        AppCommand::Strategy(StrategyCommand::History)
    );
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
    assert!(shell_help_text().contains("/portfolio"));
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
    let command_matches = complete_shell_input("/po", &[]);
    assert!(command_matches.contains(&"/portfolio".to_string()));

    let close_matches = complete_shell_input("/cl", &[]);
    assert!(close_matches.contains(&"/close-all".to_string()));
    assert!(close_matches.contains(&"/close-symbol".to_string()));

    let option_matches = complete_shell_input("/op", &[]);
    assert!(option_matches.contains(&"/option-order".to_string()));

    let strategy_matches = complete_shell_input("/str", &[]);
    assert!(strategy_matches.contains(&"/strategy".to_string()));

    let option_symbol_seed_matches = complete_shell_input(
        "/option-order ",
        &[
            "BTC-260327-200000-C".to_string(),
            "BTC-260327-100000-C".to_string(),
            "ETH-260327-5000-P".to_string(),
        ],
    );
    assert!(option_symbol_seed_matches.contains(&"/option-order BTC-".to_string()));
    assert!(option_symbol_seed_matches.contains(&"/option-order ETH-".to_string()));

    let option_contract_matches = complete_shell_input(
        "/option-order BTC-260327-2",
        &[
            "BTC-260327-200000-C".to_string(),
            "BTC-260327-100000-C".to_string(),
            "ETH-260327-5000-P".to_string(),
        ],
    );
    assert_eq!(
        option_contract_matches,
        vec!["/option-order BTC-260327-200000-C".to_string()]
    );

    let mode_matches = complete_shell_input("/mode d", &[]);
    assert_eq!(mode_matches, vec!["/mode demo".to_string()]);

    let instrument_matches = complete_shell_input(
        "/close-symbol BT",
        &["BTCUSDT".to_string(), "ETHUSDT".to_string()],
    );
    assert_eq!(
        instrument_matches,
        vec!["/close-symbol BTCUSDT".to_string()]
    );

    let fallback_matches = complete_shell_input("/set-target-exposure BTC", &[]);
    assert_eq!(
        fallback_matches,
        vec![
            "/set-target-exposure BTCUSDT".to_string(),
            "/set-target-exposure BTCUSDC".to_string(),
        ]
    );

    let weird_matches = complete_shell_input("/set-target-exposure BTCUSDTUSDCTUST", &[]);
    assert!(weird_matches.is_empty());

    let target_matches = complete_shell_input("/set-target-exposure BTCUSDT ", &[]);
    assert!(target_matches.contains(&"/set-target-exposure BTCUSDT 0.5".to_string()));
    assert!(target_matches.contains(&"/set-target-exposure BTCUSDT -0.5".to_string()));

    let order_type_matches = complete_shell_input("/set-target-exposure BTCUSDT 0.5 ", &[]);
    assert!(order_type_matches.contains(&"/set-target-exposure BTCUSDT 0.5 market".to_string()));
    assert!(order_type_matches.contains(&"/set-target-exposure BTCUSDT 0.5 limit".to_string()));

    let limit_price_matches = complete_shell_input("/set-target-exposure BTCUSDT 0.5 limit ", &[]);
    assert!(
        limit_price_matches.contains(&"/set-target-exposure BTCUSDT 0.5 limit 68000".to_string())
    );

    let priced_limit_matches = complete_shell_input_with_market_data(
        "/set-target-exposure BTCUSDT 0.5 limit ",
        &["BTCUSDT".to_string()],
        &[("BTCUSDT".to_string(), 68256.2)],
    );
    assert!(priced_limit_matches
        .iter()
        .any(|item| item.value == "/set-target-exposure BTCUSDT 0.5 limit 67910.00"));
    assert!(priced_limit_matches
        .iter()
        .any(|item| item.value == "/set-target-exposure BTCUSDT 0.5 limit 68260.00"));

    let strategy_start_matches = complete_shell_input("/strategy st", &[]);
    assert!(strategy_start_matches.contains(&"/strategy start".to_string()));

    let strategy_template_matches = complete_shell_input("/strategy start ", &[]);
    assert!(strategy_template_matches
        .contains(&"/strategy start liquidation-breakdown-short".to_string()));
}

#[test]
fn normalize_instrument_symbol_preserves_option_contract_format() {
    assert_eq!(
        normalize_instrument_symbol("btc-260327-200000-c"),
        "BTC-260327-200000-C"
    );
}

#[test]
fn shell_completion_line_marks_selected_item() {
    let line = format_completion_line(&complete_shell_input_with_description("/cl", &[]), 1);

    assert_eq!(line, "/close-all  [/close-symbol]");
}

#[test]
fn completion_index_wraps_for_up_and_down_navigation() {
    assert_eq!(next_completion_index(3, 0), 1);
    assert_eq!(next_completion_index(3, 2), 0);
    assert_eq!(previous_completion_index(3, 0), 2);
    assert_eq!(previous_completion_index(3, 2), 1);
}

#[test]
fn scroll_lines_needed_detects_terminal_bottom_overflow() {
    assert_eq!(scroll_lines_needed(5, 20, 3), 0);
    assert_eq!(scroll_lines_needed(18, 20, 3), 2);
}

#[test]
fn described_completion_includes_help_text() {
    let matches = complete_shell_input_with_description("/p", &[]);
    assert!(matches.iter().any(|item| item.value == "/portfolio"));
    assert!(matches
        .iter()
        .any(|item| item.value == "/portfolio" && item.description.contains("portfolio")));
}

#[test]
fn normalize_instrument_symbol_appends_usdt_for_base_symbol() {
    assert_eq!(normalize_instrument_symbol("btc"), "BTCUSDT");
    assert_eq!(normalize_instrument_symbol("BTCUSDC"), "BTCUSDC");
}

#[test]
fn shell_intro_panel_contains_version_mode_and_directory() {
    let panel = shell_intro_panel("demo", "~/project/sandbox-quant");

    assert!(panel.contains("Sandbox Quant"));
    assert!(panel.contains("v"));
    assert!(panel.contains("mode:"));
    assert!(panel.contains("directory: ~/project/sandbox-quant"));
}
