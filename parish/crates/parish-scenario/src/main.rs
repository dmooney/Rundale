use std::path::PathBuf;

use clap::Parser;
use parish_scenario::{Scenario, ScenarioRunner};

#[derive(Debug, Parser)]
#[command(about = "Run an asserted scenario through the shipping Parish game loop")]
struct Args {
    /// YAML scenario file to execute.
    scenario: PathBuf,
}

fn main() {
    let args = Args::parse();
    let scenario = match Scenario::from_path(&args.scenario) {
        Ok(scenario) => scenario,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let report = ScenarioRunner::rundale().run(&scenario);
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("scenario report serializes")
    );
    if !report.passed {
        std::process::exit(1);
    }
}
