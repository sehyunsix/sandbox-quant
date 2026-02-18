use sandbox_quant::strategy_catalog::StrategyCatalog;

#[test]
/// Verifies baseline strategy registry shape:
/// catalog should start with built-in config/fast/slow profiles in stable order.
fn strategy_catalog_starts_with_builtin_profiles() {
    let catalog = StrategyCatalog::new("BTCUSDT", 7, 25, 2);
    let labels = catalog.labels();
    assert_eq!(labels.len(), 3);
    assert_eq!(labels[0], "MA(Config)");
    assert_eq!(labels[1], "MA(Fast 5/20)");
    assert_eq!(labels[2], "MA(Slow 20/60)");
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
    assert_eq!(labels.len(), 4);
    assert_eq!(labels[3], custom.label);
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
        .fork_profile(fast_idx, "ETHUSDT", 9, 34, 3)
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
        .fork_profile(idx, "BNBUSDT", 11, 37, 5)
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
    assert!(built_in_delete.is_none(), "builtin profile must not be removed");

    let removed = catalog
        .remove_custom_profile(custom_idx)
        .expect("custom profile should be removed");
    assert_eq!(removed.source_tag, custom.source_tag);
    assert!(catalog.index_of_label(&custom.label).is_none());
}
