use sandbox_quant::strategy_catalog::StrategyCatalog;

#[test]
/// Verifies baseline strategy registry shape:
/// catalog should start with built-in config/fast/slow profiles in stable order.
fn strategy_catalog_starts_with_builtin_profiles() {
    let catalog = StrategyCatalog::new(7, 25, 2);
    let labels = catalog.labels();
    assert_eq!(labels.len(), 3);
    assert_eq!(labels[0], "MA(Config)");
    assert_eq!(labels[1], "MA(Fast 5/20)");
    assert_eq!(labels[2], "MA(Slow 20/60)");
}

#[test]
/// Verifies grid-created strategy registration:
/// custom profile should be appended to catalog and exposed in selectable labels.
fn strategy_catalog_registers_custom_profile() {
    let mut catalog = StrategyCatalog::new(7, 25, 2);
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
/// Verifies builtin strategy config updates:
/// editing builtins should be allowed and reflected in label/period values.
fn strategy_catalog_updates_builtin_profile_config() {
    let mut catalog = StrategyCatalog::new(7, 25, 2);
    let fast_idx = catalog
        .index_of_label("MA(Fast 5/20)")
        .expect("builtin fast should exist");
    let updated = catalog
        .update_profile(fast_idx, 9, 34, 3)
        .expect("builtin fast should update");

    assert_eq!(updated.fast_period, 9);
    assert_eq!(updated.slow_period, 34);
    assert_eq!(updated.min_ticks_between_signals, 3);
    assert_eq!(updated.label, "MA(Fast 9/34)");
}

#[test]
/// Verifies custom strategy config updates:
/// editing a registered custom strategy should update periods/cooldown and relabel it.
fn strategy_catalog_updates_custom_profile_config() {
    let mut catalog = StrategyCatalog::new(7, 25, 2);
    let custom = catalog.add_custom_from_index(0);
    let idx = catalog
        .index_of_label(&custom.label)
        .expect("custom strategy should exist");
    let updated = catalog
        .update_profile(idx, 11, 37, 5)
        .expect("custom strategy must update");

    assert_eq!(updated.fast_period, 11);
    assert_eq!(updated.slow_period, 37);
    assert_eq!(updated.min_ticks_between_signals, 5);
    assert!(updated.label.contains("11/37"));
    assert!(updated.label.contains("[c01]"));
}
