use std::fs;
use std::path::{Path, PathBuf};

use parish_scenario::{Scenario, ScenarioRunner};

fn scenario_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/scenarios")
}

#[test]
fn every_committed_scenario_parses_and_passes() {
    let mut paths = fs::read_dir(scenario_dir())
        .expect("read testing/scenarios")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert!(!paths.is_empty(), "at least one scenario must be committed");

    for path in paths {
        let scenario = Scenario::from_path(&path).expect("scenario parses and validates");
        let report = ScenarioRunner::rundale().run(&scenario);
        assert!(
            report.passed,
            "scenario {} failed:\n{}",
            path.display(),
            serde_json::to_string_pretty(&report).unwrap()
        );
    }
}
