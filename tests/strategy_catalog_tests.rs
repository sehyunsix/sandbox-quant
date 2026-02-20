use sandbox_quant::strategy_catalog::{
    strategy_kind_categories, strategy_kind_labels, strategy_kind_labels_by_category,
    strategy_type_options_by_category, StrategyCatalog, StrategyKind, StrategyProfile,
};

#[test]
/// Verifies baseline strategy registry shape:
/// catalog should start with built-in strategy profiles in stable order.
fn strategy_catalog_starts_with_builtin_profiles() {
    let catalog = StrategyCatalog::new("BTCUSDT", 7, 25, 2);
    let labels = catalog.labels();
    assert_eq!(labels.len(), 12);
    assert_eq!(labels[0], "MA(Config)");
    assert_eq!(labels[1], "MA(Fast 5/20)");
    assert_eq!(labels[2], "MA(Slow 20/60)");
    assert_eq!(labels[3], "RSA(RSI 14 30/70)");
    assert_eq!(labels[4], "DCT(Donchian 20/10)");
    assert_eq!(labels[5], "MRV(SMA 20 -2.00%)");
    assert_eq!(labels[6], "BBR(BB 20 2.00x)");
    assert_eq!(labels[7], "STO(Stoch 14 20/80)");
    assert_eq!(labels[8], "VLC(Compression 20 1.20%)");
    assert_eq!(labels[9], "ORB(Opening 12/8)");
    assert_eq!(labels[10], "REG(Regime 10/30)");
    assert_eq!(labels[11], "ENS(Vote 10/30)");
    let first = catalog.get(0).expect("builtin profile should exist");
    assert_eq!(first.symbol, "BTCUSDT");
    assert!(first.created_at_ms > 0);
}

#[test]
/// Verifies grid-created strategy registration:
/// custom profile should be appended to catalog and exposed in selectable labels.
fn strategy_catalog_registers_custom_profile() {
    let mut catalog = StrategyCatalog::new("BTCUSDT", 7, 25, 2);
    let custom = catalog.add_custom_from_index(1);

    assert!(custom.label.starts_with("MA(Custom "));
    assert_eq!(custom.source_tag, "c01");
    assert_eq!(custom.fast_period, 5);
    assert_eq!(custom.slow_period, 20);
    assert!(catalog.index_of_label(&custom.label).is_some());

    let labels = catalog.labels();
    assert_eq!(labels.len(), 13);
    assert_eq!(labels[12], custom.label);
}

#[test]
/// Verifies builtin strategy fork-on-edit behavior:
/// editing builtins should append a new custom profile and keep builtin unchanged.
fn strategy_catalog_forks_builtin_profile_config_on_edit() {
    let mut catalog = StrategyCatalog::new("BTCUSDT", 7, 25, 2);
    let fast_idx = catalog
        .index_of_label("MA(Fast 5/20)")
        .expect("builtin fast should exist");
    let forked = catalog
        .fork_profile(fast_idx, StrategyKind::Ma, "ETHUSDT", 9, 34, 3)
        .expect("builtin fast should fork");

    assert_eq!(forked.fast_period, 9);
    assert_eq!(forked.slow_period, 34);
    assert_eq!(forked.min_ticks_between_signals, 3);
    assert!(forked.label.starts_with("MA(Custom 9/34)"));
    assert_eq!(forked.source_tag, "c01");
    assert_eq!(catalog.labels()[1], "MA(Fast 5/20)");
}

#[test]
/// Verifies custom strategy fork-on-edit behavior:
/// editing a registered custom strategy should create a newer custom profile.
fn strategy_catalog_forks_custom_profile_config_on_edit() {
    let mut catalog = StrategyCatalog::new("BTCUSDT", 7, 25, 2);
    let custom = catalog.add_custom_from_index(0);
    let idx = catalog
        .index_of_label(&custom.label)
        .expect("custom strategy should exist");
    let forked = catalog
        .fork_profile(idx, StrategyKind::Ma, "BNBUSDT", 11, 37, 5)
        .expect("custom strategy must fork");

    assert_eq!(forked.fast_period, 11);
    assert_eq!(forked.slow_period, 37);
    assert_eq!(forked.min_ticks_between_signals, 5);
    assert!(forked.label.contains("11/37"));
    assert!(forked.label.contains("[c02]"));
    assert!(catalog.labels().contains(&custom.label));
    assert_eq!(forked.symbol, "BNBUSDT");
}

#[test]
/// Verifies lifecycle running-time accumulation:
/// mark_running/mark_stopped transitions should accumulate elapsed milliseconds.
fn strategy_catalog_accumulates_running_time_across_sessions() {
    let mut catalog = StrategyCatalog::new("BTCUSDT", 7, 25, 2);
    let source = catalog
        .get(0)
        .expect("builtin config should exist")
        .source_tag
        .clone();

    assert!(catalog.mark_running(&source, 1_000));
    assert!(catalog.mark_stopped(&source, 1_750));
    assert!(catalog.mark_running(&source, 2_000));
    assert!(catalog.mark_stopped(&source, 2_500));

    let profile = catalog
        .get_by_source_tag(&source)
        .expect("profile should still exist");
    assert_eq!(profile.cumulative_running_ms, 1_250);
    assert!(profile.last_started_at_ms.is_none());
}

#[test]
/// Verifies custom-only deletion rule:
/// builtins must be protected and custom profiles should be removable.
fn strategy_catalog_deletes_only_custom_profiles() {
    let mut catalog = StrategyCatalog::new("BTCUSDT", 7, 25, 2);
    let custom = catalog.add_custom_from_index(0);
    let custom_idx = catalog
        .index_of_label(&custom.label)
        .expect("custom strategy should exist");

    let built_in_delete = catalog.remove_custom_profile(0);
    assert!(
        built_in_delete.is_none(),
        "builtin profile must not be removed"
    );

    let removed = catalog
        .remove_custom_profile(custom_idx)
        .expect("custom profile should be removed");
    assert_eq!(removed.source_tag, custom.source_tag);
    assert!(catalog.index_of_label(&custom.label).is_none());
}

#[test]
/// Verifies legacy session migration behavior:
/// loading old profiles without newer builtins should auto-inject them into catalog.
fn strategy_catalog_from_profiles_injects_missing_builtin_profiles() {
    let legacy = vec![
        StrategyProfile {
            label: "MA(Config)".to_string(),
            source_tag: "cfg".to_string(),
            strategy_type: "ma".to_string(),
            symbol: "BTCUSDT".to_string(),
            created_at_ms: 1,
            cumulative_running_ms: 0,
            last_started_at_ms: None,
            fast_period: 9,
            slow_period: 21,
            min_ticks_between_signals: 2,
        },
        StrategyProfile {
            label: "MA(Fast 5/20)".to_string(),
            source_tag: "fst".to_string(),
            strategy_type: "ma".to_string(),
            symbol: "BTCUSDT".to_string(),
            created_at_ms: 1,
            cumulative_running_ms: 0,
            last_started_at_ms: None,
            fast_period: 5,
            slow_period: 20,
            min_ticks_between_signals: 2,
        },
        StrategyProfile {
            label: "MA(Slow 20/60)".to_string(),
            source_tag: "slw".to_string(),
            strategy_type: "ma".to_string(),
            symbol: "BTCUSDT".to_string(),
            created_at_ms: 1,
            cumulative_running_ms: 0,
            last_started_at_ms: None,
            fast_period: 20,
            slow_period: 60,
            min_ticks_between_signals: 2,
        },
    ];

    let catalog = StrategyCatalog::from_profiles(legacy, "BTCUSDT", 9, 21, 2);
    assert!(catalog.get_by_source_tag("rsa").is_some());
    assert!(catalog.get_by_source_tag("dct").is_some());
    assert!(catalog.get_by_source_tag("mrv").is_some());
    assert!(catalog.get_by_source_tag("bbr").is_some());
    assert!(catalog.get_by_source_tag("sto").is_some());
    assert!(catalog.get_by_source_tag("vlc").is_some());
    assert!(catalog.get_by_source_tag("orb").is_some());
    assert!(catalog.get_by_source_tag("reg").is_some());
    assert!(catalog.get_by_source_tag("ens").is_some());
    assert!(catalog.labels().iter().any(|l| l == "RSA(RSI 14 30/70)"));
    assert!(catalog.labels().iter().any(|l| l == "DCT(Donchian 20/10)"));
    assert!(catalog.labels().iter().any(|l| l == "MRV(SMA 20 -2.00%)"));
    assert!(catalog.labels().iter().any(|l| l == "BBR(BB 20 2.00x)"));
    assert!(catalog.labels().iter().any(|l| l == "STO(Stoch 14 20/80)"));
    assert!(catalog.labels().iter().any(|l| l == "VLC(Compression 20 1.20%)"));
    assert!(catalog.labels().iter().any(|l| l == "ORB(Opening 12/8)"));
    assert!(catalog.labels().iter().any(|l| l == "REG(Regime 10/30)"));
    assert!(catalog.labels().iter().any(|l| l == "ENS(Vote 10/30)"));
}

#[test]
/// Verifies strategy kind registry labels:
/// editor candidate labels should be registry-driven and include MA/RSA.
fn strategy_kind_labels_include_supported_candidates() {
    let labels = strategy_kind_labels();
    assert_eq!(
        labels,
        vec![
            "MA".to_string(),
            "EMA".to_string(),
            "ATR".to_string(),
            "VLC".to_string(),
            "CHB".to_string(),
            "ORB".to_string(),
            "RSA".to_string(),
            "DCT".to_string(),
            "MRV".to_string(),
            "BBR".to_string(),
            "STO".to_string(),
            "REG".to_string(),
            "ENS".to_string(),
        ]
    );
    assert_eq!(
        strategy_kind_categories(),
        vec![
            "Trend".to_string(),
            "MeanReversion".to_string(),
            "Volatility".to_string(),
            "Breakout".to_string(),
            "Hybrid".to_string()
        ]
    );
    assert_eq!(
        strategy_kind_labels_by_category("Trend"),
        vec!["MA".to_string(), "EMA".to_string(), "DCT".to_string()]
    );
    assert_eq!(
        strategy_kind_labels_by_category("MeanReversion"),
        vec![
            "RSA".to_string(),
            "MRV".to_string(),
            "BBR".to_string(),
            "STO".to_string()
        ]
    );
    assert_eq!(
        strategy_kind_labels_by_category("Volatility"),
        vec!["ATR".to_string(), "VLC".to_string()]
    );
    assert_eq!(
        strategy_kind_labels_by_category("Breakout"),
        vec!["CHB".to_string(), "ORB".to_string()]
    );
    let breakout_options = strategy_type_options_by_category("Breakout");
    assert!(breakout_options
        .iter()
        .any(|opt| opt.display_label == "CHB" && opt.strategy_label.as_deref() == Some("CHB")));
    assert!(breakout_options
        .iter()
        .any(|opt| opt.display_label == "ORB" && opt.strategy_label.as_deref() == Some("ORB")));
    let trend_options = strategy_type_options_by_category("Trend");
    assert!(trend_options
        .iter()
        .any(|opt| opt.display_label == "MA" && opt.strategy_label.as_deref() == Some("MA")));
    assert!(trend_options
        .iter()
        .any(|opt| opt.display_label == "EMA" && opt.strategy_label.as_deref() == Some("EMA")));
    assert!(trend_options
        .iter()
        .any(|opt| opt.display_label == "DCT" && opt.strategy_label.as_deref() == Some("DCT")));
    assert!(trend_options
        .iter()
        .all(|opt| !opt.display_label.contains("Coming soon") || opt.strategy_label.is_none()));
    let mr_options = strategy_type_options_by_category("MeanReversion");
    assert!(mr_options
        .iter()
        .any(|opt| opt.display_label == "RSA" && opt.strategy_label.as_deref() == Some("RSA")));
    assert!(mr_options
        .iter()
        .any(|opt| opt.display_label == "MRV" && opt.strategy_label.as_deref() == Some("MRV")));
    assert!(mr_options
        .iter()
        .any(|opt| opt.display_label == "BBR" && opt.strategy_label.as_deref() == Some("BBR")));
    assert!(mr_options
        .iter()
        .any(|opt| opt.display_label == "STO" && opt.strategy_label.as_deref() == Some("STO")));
    assert!(mr_options
        .iter()
        .all(|opt| !opt.display_label.contains("Coming soon") || opt.strategy_label.is_none()));
    let hybrid_options = strategy_type_options_by_category("Hybrid");
    assert!(hybrid_options
        .iter()
        .any(|opt| opt.display_label == "REG" && opt.strategy_label.as_deref() == Some("REG")));
    assert!(hybrid_options
        .iter()
        .any(|opt| opt.display_label == "ENS" && opt.strategy_label.as_deref() == Some("ENS")));
    assert_eq!(StrategyKind::from_label("ma"), Some(StrategyKind::Ma));
    assert_eq!(StrategyKind::from_label("ema"), Some(StrategyKind::Ema));
    assert_eq!(StrategyKind::from_label("atr"), Some(StrategyKind::Atr));
    assert_eq!(StrategyKind::from_label("vlc"), Some(StrategyKind::Vlc));
    assert_eq!(StrategyKind::from_label("chb"), Some(StrategyKind::Chb));
    assert_eq!(StrategyKind::from_label("orb"), Some(StrategyKind::Orb));
    assert_eq!(StrategyKind::from_label("rsa"), Some(StrategyKind::Rsa));
    assert_eq!(StrategyKind::from_label("dct"), Some(StrategyKind::Dct));
    assert_eq!(StrategyKind::from_label("mrv"), Some(StrategyKind::Mrv));
    assert_eq!(StrategyKind::from_label("bbr"), Some(StrategyKind::Bbr));
    assert_eq!(StrategyKind::from_label("sto"), Some(StrategyKind::Sto));
    assert_eq!(StrategyKind::from_label("reg"), Some(StrategyKind::Reg));
    assert_eq!(StrategyKind::from_label("ens"), Some(StrategyKind::Ens));
}

#[test]
/// Verifies strategy_type backfill on legacy profiles:
/// missing strategy_type should be inferred during from_profiles migration.
fn strategy_profile_backfills_strategy_type_on_load() {
    let legacy = vec![StrategyProfile {
        label: "RSA(RSI 14 30/70)".to_string(),
        source_tag: "rsa".to_string(),
        strategy_type: "".to_string(),
        symbol: "BTCUSDT".to_string(),
        created_at_ms: 1,
        cumulative_running_ms: 0,
        last_started_at_ms: None,
        fast_period: 14,
        slow_period: 70,
        min_ticks_between_signals: 2,
    }];
    let catalog = StrategyCatalog::from_profiles(legacy, "BTCUSDT", 9, 21, 2);
    let rsa = catalog
        .get_by_source_tag("rsa")
        .expect("rsa profile should exist");
    assert_eq!(rsa.strategy_type, "rsa");
}
