//! The bug report the iPhone app files as a GitHub issue.
//!
//! `/bug` or a shake sends this report, with a screenshot, to the
//! `limerick-bug-report` service, which files it in the repository
//! (`bug-report/`, #2022). This module turns the game's context (scene,
//! recent transcript, recent Endpoint exchanges) into one plain-text report.
//! Pure and portable: no network, no credentials, no clock. Plan:
//! `docs/plans/mobile-bug-report.md`.
//!
//! Lengths are counted in Unicode scalar values (`char`s), not bytes, so an
//! Irish name such as "Mícheál" costs what the reader sees.

/// The report's ceiling. The service wraps it in an issue body whose own cap
/// is 90% of GitHub's 65,536-character limit (58,982), per the external
/// payload rule (`docs/agent/test-tooling-rules.md`); this leaves room for the
/// description and screenshot link it adds.
pub const REPORT_BUDGET: usize = 50_000;
/// The most characters of the player's own description the report keeps.
pub const DESCRIPTION_LIMIT: usize = 2_000;
/// The most characters one transcript line keeps.
const TRANSCRIPT_LINE_LIMIT: usize = 1_000;
/// The most characters of what an exchange asked the report keeps.
const EXCHANGE_ASKED_LIMIT: usize = 1_000;
/// The most characters of an exchange's reply or failure the report keeps.
const EXCHANGE_OUTPUT_LIMIT: usize = 4_000;

/// Everything a report can say, gathered by the host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MobileReport {
    /// What the player typed after `/bug`; may be empty.
    pub description: String,
    /// The app's version and build, when the host knows it.
    pub build: Option<String>,
    /// The engine's wire contract version.
    pub contract_version: String,
    /// The player's location name.
    pub scene: String,
    /// Time-of-day label.
    pub time_of_day: String,
    /// Weather label.
    pub weather: String,
    /// Who is present, as the player knows them.
    pub present: Vec<String>,
    /// What the open request is doing, if one is open.
    pub open_request: Option<String>,
    /// Recent transcript lines, oldest first.
    pub transcript: Vec<TranscriptLine>,
    /// Recent Endpoint exchanges, oldest first.
    pub exchanges: Vec<ExchangeRecord>,
}

/// One transcript line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptLine {
    /// Who said it: `None` for narration and system lines.
    pub speaker: Option<String>,
    /// Whether the player typed it.
    pub from_player: bool,
    /// The text.
    pub text: String,
}

/// One answered Endpoint call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeRecord {
    /// Endpoint slug and version, for example `rundale-dialogue.v1`.
    pub endpoint: String,
    /// Time from the call being handed to the host to its answer.
    pub duration_ms: Option<u64>,
    /// What was asked, in brief: the player's words, and for dialogue who
    /// was asked and where.
    pub asked: String,
    /// How it ended.
    pub outcome: ExchangeOutcome,
}

/// How an Endpoint call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeOutcome {
    /// The Endpoint answered with this output.
    Completed {
        /// The output (compact JSON).
        output: String,
    },
    /// The host reported a failure.
    Failed {
        /// Failure kind and, when given, its reason (`transport/offline`).
        kind: String,
        /// The diagnostic message.
        message: String,
    },
}

/// Composes the report, at most `budget` characters long.
///
/// The header (description, build, scene, open request) comes first. The
/// remaining space is shared by the transcript and the exchanges, each
/// filled newest first and rendered oldest first, so the moments closest to
/// the report survive and older ones are dropped. Each line has its own cap,
/// so one long reply cannot crowd out the rest.
pub fn compose(report: &MobileReport, budget: usize) -> String {
    let head = render_head(report);
    let transcript: Vec<String> = report.transcript.iter().map(render_line).collect();
    let exchanges: Vec<String> = report.exchanges.iter().map(render_exchange).collect();

    let transcript_title = "\nTranscript (newest last):\n";
    let exchanges_title = "\nEndpoint calls since launch (newest last):\n";
    let no_exchanges = "\nEndpoint calls since launch: none\n";
    let fixed = chars(&head)
        + if transcript.is_empty() {
            0
        } else {
            chars(transcript_title)
        }
        + if exchanges.is_empty() {
            chars(no_exchanges)
        } else {
            chars(exchanges_title)
        };
    let mut remaining = budget.saturating_sub(fixed);

    // Half the space to the exchanges first, then the transcript takes what
    // is left, then the exchanges take whatever the transcript did not use.
    let (mut exchange_count, exchange_used) = newest_fitting(&exchanges, remaining / 2);
    remaining -= exchange_used;
    let (transcript_count, transcript_used) = newest_fitting(&transcript, remaining);
    remaining -= transcript_used;
    let (more, _) = newest_fitting(&exchanges[..exchanges.len() - exchange_count], remaining);
    exchange_count += more;

    let mut out = head;
    if !transcript.is_empty() {
        out.push_str(transcript_title);
        for line in &transcript[transcript.len() - transcript_count..] {
            out.push_str(line);
        }
    }
    if exchanges.is_empty() {
        out.push_str(no_exchanges);
    } else {
        out.push_str(exchanges_title);
        for line in &exchanges[exchanges.len() - exchange_count..] {
            out.push_str(line);
        }
    }
    clip_chars(&out, budget)
}

fn render_head(report: &MobileReport) -> String {
    let description = one_line(&report.description);
    let description = if description.is_empty() {
        "(no description)".to_string()
    } else {
        clip(&description, DESCRIPTION_LIMIT)
    };
    let mut head = format!("Rundale bug report\n{description}\n\n");
    let build = report.build.as_deref().unwrap_or("unknown build");
    head.push_str(&format!(
        "Build: {build} · engine contract {}\n",
        report.contract_version
    ));
    head.push_str(&format!(
        "Scene: {} · {} · {}\n",
        report.scene, report.time_of_day, report.weather
    ));
    let present = if report.present.is_empty() {
        "no one".to_string()
    } else {
        report.present.join(", ")
    };
    head.push_str(&format!("Present: {present}\n"));
    if let Some(open) = &report.open_request {
        head.push_str(&format!("Open request: {open}\n"));
    }
    head
}

fn render_line(line: &TranscriptLine) -> String {
    let text = clip(&one_line(&line.text), TRANSCRIPT_LINE_LIMIT);
    match (&line.speaker, line.from_player) {
        (_, true) => format!("> {text}\n"),
        (Some(speaker), false) => format!("{speaker}: {text}\n"),
        (None, false) => format!("* {text}\n"),
    }
}

fn render_exchange(exchange: &ExchangeRecord) -> String {
    let took = exchange
        .duration_ms
        .map(|ms| format!(" {ms}ms"))
        .unwrap_or_default();
    let asked = clip(&one_line(&exchange.asked), EXCHANGE_ASKED_LIMIT);
    match &exchange.outcome {
        ExchangeOutcome::Completed { output } => format!(
            "- {}{took} ok\n  asked: {asked}\n  reply: {}\n",
            exchange.endpoint,
            clip(&one_line(output), EXCHANGE_OUTPUT_LIMIT)
        ),
        ExchangeOutcome::Failed { kind, message } => format!(
            "- {}{took} failed ({kind})\n  asked: {asked}\n  error: {}\n",
            exchange.endpoint,
            clip(&one_line(message), EXCHANGE_OUTPUT_LIMIT)
        ),
    }
}

/// How many of the newest `lines` fit in `budget` characters, and the
/// characters they use.
fn newest_fitting(lines: &[String], budget: usize) -> (usize, usize) {
    let mut used = 0;
    let mut count = 0;
    for line in lines.iter().rev() {
        let cost = chars(line);
        if used + cost > budget {
            break;
        }
        used += cost;
        count += 1;
    }
    (count, used)
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// Collapses runs of whitespace (including newlines) into single spaces.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Keeps the first `max` characters, ending with `…` when cut.
fn clip(text: &str, max: usize) -> String {
    if chars(text) <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// The hard cap: the first `max` characters.
fn clip_chars(text: &str, max: usize) -> String {
    if chars(text) <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A budget small enough that trimming shows.
    const SMALL: usize = 3_600;

    fn report() -> MobileReport {
        MobileReport {
            description: "the miller ignored me".to_string(),
            build: Some("0.1 (42)".to_string()),
            contract_version: "3".to_string(),
            scene: "Kilteevan Village".to_string(),
            time_of_day: "Morning".to_string(),
            weather: "Clear".to_string(),
            present: vec!["Peig".to_string()],
            open_request: None,
            transcript: vec![
                TranscriptLine {
                    speaker: None,
                    from_player: true,
                    text: "Mícheál, how are the cattle?".to_string(),
                },
                TranscriptLine {
                    speaker: Some("Peig".to_string()),
                    from_player: false,
                    text: "He is not here.".to_string(),
                },
            ],
            exchanges: vec![ExchangeRecord {
                endpoint: "rundale-intent.v1".to_string(),
                duration_ms: Some(812),
                asked: "Mícheál, how are the cattle?".to_string(),
                outcome: ExchangeOutcome::Failed {
                    kind: "transport/timed_out".to_string(),
                    message: "request timed out".to_string(),
                },
            }],
        }
    }

    #[test]
    fn small_report_renders_every_section_in_order() {
        let text = compose(&report(), REPORT_BUDGET);
        let expected = "Rundale bug report\n\
            the miller ignored me\n\n\
            Build: 0.1 (42) · engine contract 3\n\
            Scene: Kilteevan Village · Morning · Clear\n\
            Present: Peig\n\
            \nTranscript (newest last):\n\
            > Mícheál, how are the cattle?\n\
            Peig: He is not here.\n\
            \nEndpoint calls since launch (newest last):\n\
            - rundale-intent.v1 812ms failed (transport/timed_out)\n  \
            asked: Mícheál, how are the cattle?\n  \
            error: request timed out\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn empty_description_and_no_exchanges_are_stated() {
        let mut report = report();
        report.description = "   ".to_string();
        report.exchanges.clear();
        report.present.clear();
        report.open_request = Some("awaiting rundale-dialogue.v1".to_string());
        let text = compose(&report, REPORT_BUDGET);
        assert!(text.contains("Rundale bug report\n(no description)\n"));
        assert!(text.contains("Present: no one\n"));
        assert!(text.contains("Open request: awaiting rundale-dialogue.v1\n"));
        assert!(text.ends_with("\nEndpoint calls since launch: none\n"));
    }

    // The service's issue body (the report, the description again, and the
    // screenshot link) stays under 90% of GitHub's 65,536-character limit.
    const _: () = assert!(REPORT_BUDGET + DESCRIPTION_LIMIT + 2_000 <= 58_982);

    #[test]
    fn long_history_fits_the_budget_and_keeps_the_newest() {
        let mut report = report();
        report.transcript = (0..200)
            .map(|n| TranscriptLine {
                speaker: Some("Peig".to_string()),
                from_player: false,
                text: format!("line {n} {}", "á".repeat(150)),
            })
            .collect();
        report.exchanges = (0..30)
            .map(|n| ExchangeRecord {
                endpoint: format!("rundale-dialogue.v{n}"),
                duration_ms: Some(n),
                asked: "{}".repeat(200),
                outcome: ExchangeOutcome::Completed {
                    output: "é".repeat(500),
                },
            })
            .collect();
        let text = compose(&report, SMALL);
        assert!(text.chars().count() <= SMALL, "{}", text.chars().count());
        assert!(text.contains("line 199 "), "newest transcript line kept");
        assert!(!text.contains("line 0 "), "oldest transcript line dropped");
        assert!(
            text.contains("rundale-dialogue.v29 "),
            "newest exchange kept"
        );
        assert!(
            !text.contains("rundale-dialogue.v0 "),
            "oldest exchange dropped"
        );
        assert!(text.contains("the miller ignored me"), "description kept");
        let first = text.find("line 19").expect("a kept line");
        let last = text.find("line 199 ").expect("newest line");
        assert!(first <= last, "rendered oldest first");
    }

    #[test]
    fn long_lines_are_clipped_and_flattened() {
        let mut report = report();
        report.description = format!("first\nsecond {}", "x".repeat(2_000));
        report.transcript = vec![TranscriptLine {
            speaker: None,
            from_player: false,
            text: format!("a\n\nb {}", "y".repeat(1_000)),
        }];
        let text = compose(&report, REPORT_BUDGET);
        let description = text.lines().nth(1).expect("description line");
        assert!(description.starts_with("first second "));
        assert_eq!(description.chars().count(), DESCRIPTION_LIMIT);
        assert!(description.ends_with('…'));
        let line = text
            .lines()
            .find(|line| line.starts_with("* a b "))
            .expect("narration line");
        assert_eq!(line.chars().count(), 2 + TRANSCRIPT_LINE_LIMIT);
    }

    #[test]
    fn unused_transcript_space_goes_to_older_exchanges() {
        let mut report = report();
        report.transcript.clear();
        report.exchanges = (0..12)
            .map(|n| ExchangeRecord {
                endpoint: format!("rundale-dialogue.v{n}"),
                duration_ms: None,
                asked: "i".repeat(150),
                outcome: ExchangeOutcome::Completed {
                    output: "o".repeat(250),
                },
            })
            .collect();
        let text = compose(&report, SMALL);
        let kept = text.matches("- rundale-dialogue.v").count();
        // Each exchange costs about 430 characters, so half the budget holds
        // three or four; the transcript's unused half must hold more.
        assert!(kept >= 7, "kept {kept}");
        assert!(text.chars().count() <= SMALL);
    }

    #[test]
    fn a_tiny_budget_is_still_respected() {
        let text = compose(&report(), 40);
        assert_eq!(text.chars().count(), 40);
        assert!(text.starts_with("Rundale bug report\n"));
    }
}
