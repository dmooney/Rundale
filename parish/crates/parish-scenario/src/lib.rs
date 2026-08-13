//! Versioned, machine-asserted gameplay scenarios over the shipping game loop.
//!
//! The runner uses [`parish_engine::testing::GameTestHarness`] only as a state
//! container and calls `execute_via_real_loop` for every step. Inference is the
//! sole mocked boundary; command routing, state mutation, and emitted IPC
//! events come from `parish_core::game_loop`.

use std::fs;
use std::path::Path;

use parish_engine::testing::GameTestHarness;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("read scenario {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("parse scenario {path}: {source}")]
    Parse {
        path: String,
        source: serde_yaml::Error,
    },
    #[error("scenario {name:?} uses schema version {actual}; supported version is {expected}")]
    UnsupportedVersion {
        name: String,
        actual: u32,
        expected: u32,
    },
    #[error("invalid scenario {name:?}: {detail}")]
    Invalid { name: String, detail: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<Step>,
}

impl Scenario {
    pub fn from_path(path: &Path) -> Result<Self, ScenarioError> {
        let source = fs::read_to_string(path).map_err(|source| ScenarioError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let scenario: Self =
            serde_yaml::from_str(&source).map_err(|source| ScenarioError::Parse {
                path: path.display().to_string(),
                source,
            })?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.version != SCHEMA_VERSION {
            return Err(ScenarioError::UnsupportedVersion {
                name: self.name.clone(),
                actual: self.version,
                expected: SCHEMA_VERSION,
            });
        }
        if self.name.trim().is_empty() {
            return self.invalid("name must not be empty");
        }
        if self.steps.is_empty() {
            return self.invalid("at least one step is required");
        }
        for (index, step) in self.steps.iter().enumerate() {
            let step_number = index + 1;
            if step.input.trim().is_empty() {
                return self.invalid(format!("step {step_number} input must not be empty"));
            }
            if step.expect.min_events == Some(0) {
                return self.invalid(format!(
                    "step {step_number} min_events must require at least one event"
                ));
            }
            if step
                .expect
                .absent_events
                .iter()
                .any(|name| name.trim().is_empty())
            {
                return self.invalid(format!(
                    "step {step_number} absent event name must not be empty"
                ));
            }
            for event in &step.expect.absent_event_text {
                if event.name.trim().is_empty() {
                    return self.invalid(format!(
                        "step {step_number} absent event text name must not be empty"
                    ));
                }
                if event.contains.trim().is_empty() {
                    return self.invalid(format!(
                        "step {step_number} absent event text substring must not be empty"
                    ));
                }
            }
            if !step.expect.has_oracle() {
                return self.invalid(format!(
                    "step {step_number} must contain at least one machine assertion"
                ));
            }
            for completion in &step.mock {
                if completion.raw_json && completion.prompt_contains.is_none() {
                    return self.invalid(format!(
                        "step {step_number} raw_json completion requires prompt_contains"
                    ));
                }
            }
            for event in &step.expect.events {
                if event.name.trim().is_empty() {
                    return self
                        .invalid(format!("step {step_number} event name must not be empty"));
                }
                if event.at_least == 0 {
                    return self.invalid(format!(
                        "step {step_number} event {:?} must require at least one match",
                        event.name
                    ));
                }
                if event.equals.is_some() && event.json_pointer.is_none() {
                    return self.invalid(format!(
                        "step {step_number} event {:?} uses equals without json_pointer",
                        event.name
                    ));
                }
            }
        }
        Ok(())
    }

    fn invalid(&self, detail: impl Into<String>) -> Result<(), ScenarioError> {
        Err(ScenarioError::Invalid {
            name: self.name.clone(),
            detail: detail.into(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub input: String,
    #[serde(default)]
    pub mock: Vec<MockCompletion>,
    pub expect: Expectation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MockCompletion {
    #[serde(default)]
    pub prompt_contains: Option<String>,
    pub response: String,
    #[serde(default)]
    pub raw_json: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub clock: Option<String>,
    #[serde(default)]
    pub paused: Option<bool>,
    #[serde(default)]
    pub min_events: Option<usize>,
    #[serde(default)]
    pub events: Vec<EventExpectation>,
    #[serde(default)]
    pub absent_events: Vec<String>,
    #[serde(default)]
    pub absent_event_text: Vec<AbsentEventTextExpectation>,
}

impl Expectation {
    fn has_oracle(&self) -> bool {
        self.location.is_some()
            || self.clock.is_some()
            || self.paused.is_some()
            || self.min_events.is_some()
            || !self.events.is_empty()
            || !self.absent_events.is_empty()
            || !self.absent_event_text.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbsentEventTextExpectation {
    pub name: String,
    pub contains: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventExpectation {
    pub name: String,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub json_pointer: Option<String>,
    #[serde(default)]
    pub equals: Option<Value>,
    #[serde(default = "one")]
    pub at_least: usize,
}

const fn one() -> usize {
    1
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioReport {
    pub schema_version: u32,
    pub name: String,
    pub passed: bool,
    pub steps: Vec<StepReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepReport {
    pub index: usize,
    pub input: String,
    pub location: String,
    pub clock: String,
    pub paused: bool,
    pub passed: bool,
    pub failures: Vec<String>,
    pub events: Vec<EventRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRecord {
    pub name: String,
    pub payload: Value,
}

pub struct ScenarioRunner {
    harness: GameTestHarness,
}

impl ScenarioRunner {
    pub fn rundale() -> Self {
        Self {
            harness: GameTestHarness::new(),
        }
    }

    pub fn run(&mut self, scenario: &Scenario) -> ScenarioReport {
        let steps = scenario
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| self.run_step(index + 1, step))
            .collect::<Vec<_>>();
        ScenarioReport {
            schema_version: scenario.version,
            name: scenario.name.clone(),
            passed: steps.iter().all(|step| step.passed),
            steps,
        }
    }

    fn run_step(&mut self, index: usize, step: &Step) -> StepReport {
        for completion in &step.mock {
            let mock = self.harness.mock();
            match (&completion.prompt_contains, completion.raw_json) {
                (Some(needle), true) => {
                    mock.push_json_for(needle, completion.response.clone());
                }
                (Some(needle), false) => mock.push_for(needle, completion.response.clone()),
                (None, _) => mock.push_any(completion.response.clone()),
            }
        }

        let events = self
            .harness
            .execute_via_real_loop(&step.input)
            .into_iter()
            .map(|(name, payload)| EventRecord { name, payload })
            .collect::<Vec<_>>();
        let location = self.harness.player_location().to_string();
        let clock = self
            .harness
            .app
            .world
            .clock
            .now()
            .format("%H:%M")
            .to_string();
        let paused = self.harness.app.world.clock.is_paused();
        let failures = evaluate(&step.expect, &events, &location, &clock, paused);

        StepReport {
            index,
            input: step.input.clone(),
            location,
            clock,
            paused,
            passed: failures.is_empty(),
            failures,
            events,
        }
    }
}

fn evaluate(
    expect: &Expectation,
    events: &[EventRecord],
    location: &str,
    clock: &str,
    paused: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    if let Some(expected) = &expect.location
        && location != expected
    {
        failures.push(format!(
            "expected location {expected:?}, observed {location:?}"
        ));
    }
    if let Some(expected) = &expect.clock
        && clock != expected
    {
        failures.push(format!("expected clock {expected:?}, observed {clock:?}"));
    }
    if let Some(expected) = expect.paused
        && paused != expected
    {
        failures.push(format!("expected paused={expected}, observed {paused}"));
    }
    if let Some(minimum) = expect.min_events
        && events.len() < minimum
    {
        failures.push(format!(
            "expected at least {minimum} events, observed {}",
            events.len()
        ));
    }
    for forbidden in &expect.absent_events {
        if events.iter().any(|event| event.name == *forbidden) {
            failures.push(format!("forbidden event {forbidden:?} was emitted"));
        }
    }
    for forbidden in &expect.absent_event_text {
        if events.iter().any(|event| {
            event.name == forbidden.name && event.payload.to_string().contains(&forbidden.contains)
        }) {
            failures.push(format!(
                "forbidden text {:?} was emitted by event {:?}",
                forbidden.contains, forbidden.name
            ));
        }
    }
    for expected in &expect.events {
        let count = events
            .iter()
            .filter(|event| event_matches(event, expected))
            .count();
        if count < expected.at_least {
            failures.push(format!(
                "expected event {:?} at least {} time(s), observed {count}; constraint={expected:?}",
                expected.name, expected.at_least
            ));
        }
    }
    failures
}

fn event_matches(event: &EventRecord, expected: &EventExpectation) -> bool {
    if event.name != expected.name {
        return false;
    }
    if let Some(needle) = &expected.contains
        && !event.payload.to_string().contains(needle)
    {
        return false;
    }
    if let Some(pointer) = &expected.json_pointer {
        let Some(actual) = event.payload.pointer(pointer) else {
            return false;
        };
        if let Some(value) = &expected.equals
            && actual != value
        {
            return false;
        }
    } else if expected.equals.is_some() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_schema_fields() {
        let error =
            serde_yaml::from_str::<Scenario>("version: 1\nname: bad\nunknown: true\nsteps: []\n")
                .expect_err("unknown field must fail");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn event_constraints_are_machine_checked() {
        let events = vec![EventRecord {
            name: "text-log".into(),
            payload: serde_json::json!({"content": "Kilteevan wakes"}),
        }];
        let expected = Expectation {
            location: Some("Kilteevan Village".into()),
            events: vec![EventExpectation {
                name: "text-log".into(),
                contains: Some("Kilteevan".into()),
                json_pointer: Some("/content".into()),
                equals: Some(Value::String("Kilteevan wakes".into())),
                at_least: 1,
            }],
            ..Expectation::default()
        };
        assert!(evaluate(&expected, &events, "Kilteevan Village", "08:00", false).is_empty());
        assert!(!evaluate(&expected, &events, "The Crossroads", "08:00", false).is_empty());
    }

    #[test]
    fn absent_event_text_constraints_are_machine_checked() {
        let events = vec![
            EventRecord {
                name: "text-log".into(),
                payload: serde_json::json!({"content": "Taobhán is remembered here"}),
            },
            EventRecord {
                name: "stream-token".into(),
                payload: serde_json::json!({"token": "Taobhán"}),
            },
        ];
        let expected = Expectation {
            absent_event_text: vec![AbsentEventTextExpectation {
                name: "text-log".into(),
                contains: "Taobhán".into(),
            }],
            ..Expectation::default()
        };

        let failures = evaluate(&expected, &events, "Kilteevan Village", "08:00", true);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("Taobhán"));

        let expected = Expectation {
            absent_event_text: vec![AbsentEventTextExpectation {
                name: "text-log".into(),
                contains: "Good People".into(),
            }],
            ..Expectation::default()
        };
        assert!(evaluate(&expected, &events, "Kilteevan Village", "08:00", true).is_empty());
    }

    #[test]
    fn validation_rejects_scenarios_without_real_oracles() {
        let scenario = Scenario {
            version: SCHEMA_VERSION,
            name: "vacuous".into(),
            description: String::new(),
            steps: vec![Step {
                input: "look".into(),
                mock: Vec::new(),
                expect: Expectation::default(),
            }],
        };
        assert!(matches!(
            scenario.validate(),
            Err(ScenarioError::Invalid { .. })
        ));
    }

    #[test]
    fn validation_rejects_zero_minimum_as_a_vacuous_oracle() {
        let scenario = Scenario {
            version: SCHEMA_VERSION,
            name: "zero minimum".into(),
            description: String::new(),
            steps: vec![Step {
                input: "look".into(),
                mock: Vec::new(),
                expect: Expectation {
                    min_events: Some(0),
                    ..Expectation::default()
                },
            }],
        };
        assert!(matches!(
            scenario.validate(),
            Err(ScenarioError::Invalid { .. })
        ));
    }

    #[test]
    fn validation_rejects_blank_absent_event_as_a_vacuous_oracle() {
        let scenario = Scenario {
            version: SCHEMA_VERSION,
            name: "blank absent event".into(),
            description: String::new(),
            steps: vec![Step {
                input: "look".into(),
                mock: Vec::new(),
                expect: Expectation {
                    absent_events: vec!["  ".into()],
                    ..Expectation::default()
                },
            }],
        };
        assert!(matches!(
            scenario.validate(),
            Err(ScenarioError::Invalid { .. })
        ));
    }

    #[test]
    fn validation_rejects_blank_absent_event_text_constraints() {
        for (name, contains) in [("", "Taobhán"), ("text-log", "  ")] {
            let scenario = Scenario {
                version: SCHEMA_VERSION,
                name: "blank absent event text".into(),
                description: String::new(),
                steps: vec![Step {
                    input: "look".into(),
                    mock: Vec::new(),
                    expect: Expectation {
                        absent_event_text: vec![AbsentEventTextExpectation {
                            name: name.into(),
                            contains: contains.into(),
                        }],
                        ..Expectation::default()
                    },
                }],
            };
            assert!(matches!(
                scenario.validate(),
                Err(ScenarioError::Invalid { .. })
            ));
        }
    }

    #[test]
    fn validation_rejects_unmatchable_event_constraints() {
        let scenario = Scenario {
            version: SCHEMA_VERSION,
            name: "bad event".into(),
            description: String::new(),
            steps: vec![Step {
                input: "look".into(),
                mock: Vec::new(),
                expect: Expectation {
                    events: vec![EventExpectation {
                        name: "text-log".into(),
                        contains: None,
                        json_pointer: None,
                        equals: Some(Value::String("impossible".into())),
                        at_least: 1,
                    }],
                    ..Expectation::default()
                },
            }],
        };
        assert!(matches!(
            scenario.validate(),
            Err(ScenarioError::Invalid { .. })
        ));
    }
}
