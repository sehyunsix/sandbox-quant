use sandbox_quant::doctor::{parse_doctor_args, resolve_unrealized_pnl, DoctorCommand};

#[test]
fn parse_doctor_auth_json() {
    let args = vec![
        "sandbox-quant".to_string(),
        "doctor".to_string(),
        "auth".to_string(),
        "--json".to_string(),
    ];
    let cmd = parse_doctor_args(&args)
        .expect("parse should succeed")
        .expect("doctor command expected");
    assert_eq!(cmd, DoctorCommand::Auth { json: true });
}

#[test]
fn parse_doctor_positions_with_options() {
    let args = vec![
        "sandbox-quant".to_string(),
        "doctor".to_string(),
        "positions".to_string(),
        "--market".to_string(),
        "futures".to_string(),
        "--symbol".to_string(),
        "btcusdt".to_string(),
        "--json".to_string(),
    ];
    let cmd = parse_doctor_args(&args)
        .expect("parse should succeed")
        .expect("doctor command expected");
    assert_eq!(
        cmd,
        DoctorCommand::Positions {
            market: "futures".to_string(),
            symbol: Some("BTCUSDT".to_string()),
            json: true,
        }
    );
}

#[test]
fn parse_non_doctor_args_returns_none() {
    let args = vec!["sandbox-quant".to_string()];
    let cmd = parse_doctor_args(&args).expect("parse should succeed");
    assert!(cmd.is_none());
}

#[test]
fn parse_doctor_help() {
    let args = vec![
        "sandbox-quant".to_string(),
        "doctor".to_string(),
        "help".to_string(),
    ];
    let cmd = parse_doctor_args(&args)
        .expect("parse should succeed")
        .expect("doctor command expected");
    assert_eq!(cmd, DoctorCommand::Help);
}

#[test]
fn parse_doctor_history_spot_symbol_json() {
    let args = vec![
        "sandbox-quant".to_string(),
        "doctor".to_string(),
        "history".to_string(),
        "--market".to_string(),
        "spot".to_string(),
        "--symbol".to_string(),
        "ethusdt".to_string(),
        "--json".to_string(),
    ];
    let cmd = parse_doctor_args(&args)
        .expect("parse should succeed")
        .expect("doctor command expected");
    assert_eq!(
        cmd,
        DoctorCommand::History {
            market: "spot".to_string(),
            symbol: Some("ETHUSDT".to_string()),
            json: true,
        }
    );
}

#[test]
fn parse_doctor_sync_once() {
    let args = vec![
        "sandbox-quant".to_string(),
        "doctor".to_string(),
        "sync".to_string(),
        "--once".to_string(),
        "--market".to_string(),
        "futures".to_string(),
    ];
    let cmd = parse_doctor_args(&args)
        .expect("parse should succeed")
        .expect("doctor command expected");
    assert_eq!(
        cmd,
        DoctorCommand::SyncOnce {
            market: "futures".to_string(),
            symbol: None,
            json: false,
        }
    );
}

#[test]
fn resolve_unrealized_prefers_api_value() {
    let (v, source) = resolve_unrealized_pnl(1.25, 100.0, 99.0, 2.0);
    assert!((v - 1.25).abs() < 1e-9);
    assert_eq!(source, "api_unRealizedProfit");
}

#[test]
fn resolve_unrealized_falls_back_to_mark_entry_qty() {
    let (v, source) = resolve_unrealized_pnl(0.0, 105.0, 100.0, -2.0);
    assert!((v - (-10.0)).abs() < 1e-9);
    assert_eq!(source, "fallback_mark_minus_entry_times_qty");
}
