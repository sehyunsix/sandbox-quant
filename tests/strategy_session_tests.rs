use std::time::{SystemTime, UNIX_EPOCH};

use sandbox_quant::strategy_catalog::StrategyCatalog;
use sandbox_quant::strategy_session::{
    load_strategy_session_from_path, persist_strategy_session_to_path,
};

fn temp_session_path(test_name: &str) -> std::path::PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!("sq-{}-{}.json", test_name, ts))
}

#[test]
/// Verifies strategy session persistence round-trip:
/// custom strategy rows and selected source-tag should survive save->load.
fn strategy_session_round_trip_persists_catalog_and_selected_profile() {
    let path = temp_session_path("strategy-session-roundtrip");
    let mut catalog = StrategyCatalog::new("BTCUSDT", 9, 21, 2);
    let custom = catalog.add_custom_from_index(0);
    let custom_index = catalog
        .index_of_label(&custom.label)
        .expect("custom strategy index should exist");
    let forked = catalog
        .fork_profile(custom_index, "ETHUSDT", 8, 29, 3)
        .expect("custom strategy should be forkable");

    persist_strategy_session_to_path(&path, &catalog, &forked.source_tag)
        .expect("session persist should succeed");

    let loaded = load_strategy_session_from_path(&path, "BTCUSDT", 9, 21, 2)
        .expect("session load should succeed")
        .expect("persisted session should exist");

    assert_eq!(loaded.selected_source_tag.as_deref(), Some("c02"));
    assert!(loaded
        .catalog
        .get_by_source_tag("c02")
        .map(|profile| profile.label.starts_with("MA(Custom 8/29)") && profile.symbol == "ETHUSDT")
        .unwrap_or(false));
}

#[test]
/// Verifies missing-file behavior:
/// loading from a non-existent strategy session file should return Ok(None).
fn strategy_session_missing_file_returns_none() {
    let path = temp_session_path("strategy-session-missing");
    let loaded = load_strategy_session_from_path(&path, "BTCUSDT", 10, 30, 1)
        .expect("load should succeed for missing path");
    assert!(loaded.is_none());
}
