use std::fs;
use std::path::PathBuf;

use sandbox_quant::ui_docs::{
    load_scenarios_from_dir, render_to_text, run_scenarios_and_write, seed_state, Scenario,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sq-ui-docs-{}-{}", name, uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn load_scenario_toml_from_directory() {
    let dir = temp_dir("parse");
    let scenario = dir.join("001-test.toml");
    fs::write(
        &scenario,
        r#"
id = "test-id"
title = "Test Scenario"
profiles = ["smoke"]
width = 120
height = 40

[[step]]
type = "key"
value = "g"
"#,
    )
    .expect("write scenario");

    let loaded = load_scenarios_from_dir(&dir).expect("load scenarios");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "test-id");
    assert_eq!(loaded[0].title, "Test Scenario");
    assert_eq!(loaded[0].steps.len(), 1);
}

#[test]
fn run_scenario_and_update_readme_markers() {
    let dir = temp_dir("run");
    let snapshot_path = dir.join("snapshot.txt");
    let index_path = dir.join("INDEX.md");
    let readme_path = dir.join("README.md");
    fs::write(
        &readme_path,
        "head\n<!-- UI_DOCS:START -->\nold\n<!-- UI_DOCS:END -->\nfoot\n",
    )
    .expect("write readme");

    let scenario = Scenario {
        id: "dashboard-test".to_string(),
        title: "Dashboard Test".to_string(),
        width: 120,
        height: 40,
        profiles: vec!["smoke".to_string()],
        steps: vec![sandbox_quant::ui_docs::Step::Snapshot {
            path: snapshot_path.to_string_lossy().to_string(),
        }],
    };

    let rendered =
        run_scenarios_and_write(&[scenario], &index_path, &readme_path).expect("run ui docs");
    assert_eq!(rendered.len(), 1);
    assert!(snapshot_path.exists(), "snapshot should be generated");
    let svg_path = snapshot_path.with_extension("svg");
    assert!(svg_path.exists(), "svg preview should be generated");
    let index = fs::read_to_string(index_path).expect("read index");
    assert!(index.contains("Dashboard Test"));
    assert!(index.contains("!["), "index should embed image preview");
    let readme = fs::read_to_string(readme_path).expect("read readme");
    assert!(readme.contains("UI Docs (Auto)"));
    assert!(readme.contains("docs/ui/INDEX.md"));
    assert!(
        readme.contains(".svg"),
        "readme should include image preview paths"
    );
}

#[test]
fn seed_state_renders_chart_and_latency() {
    let state = seed_state();
    let text = render_to_text(&state, 160, 42).expect("render seed state");
    assert!(
        !text.contains("Waiting for data..."),
        "seed state should include candles for chart rendering"
    );
    assert!(
        text.contains("lat:180ms"),
        "status bar should expose seeded price latency"
    );
}
