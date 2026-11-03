//! Report rendering for assay runs: pure functions from outcomes to the exact stdout text.
//!
//! The run builds a typed [`SuiteReport`](crate::run::SuiteReport); the edge renders it with
//! [`render_report`] and prints the text. Output is produced once the run is complete (cases run
//! in parallel, the report is sorted by name).

use std::fmt::Write as _;
use std::io::{self, Write};

use serde::Serialize;

use crate::outcome::AssayOutcome;
use crate::run::RunSummary;
use crate::verdict::InfraError;

/// Output format for assay run reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportFormat {
  /// Human-readable lines (the default).
  #[default]
  Human,
  /// A JSON document.
  Json,
  /// TAP (Test Anything Protocol).
  Tap,
}

impl ReportFormat {
  /// Parse a CLI `--format` value (`human`, `json`, `tap`).
  pub fn parse(s: &str) -> Option<Self> {
    match s.to_ascii_lowercase().as_str() {
      "human" => Some(Self::Human),
      "json" => Some(Self::Json),
      "tap" => Some(Self::Tap),
      _ => None,
    }
  }
}

/// One formatted output line (or block) of a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportLine {
  /// The rendered text.
  pub body: String,
}

#[derive(Serialize)]
struct JsonOutcome<'a> {
  name: &'a str,
  outcome: &'a AssayOutcome,
}

/// The line for one outcome (`index` is 1-based, used by TAP). JSON reports are one document
/// ([`render_report`]); here a JSON line is the case's compact row.
pub fn format_line(
  index: usize,
  name: &str,
  outcome: &AssayOutcome,
  format: ReportFormat,
) -> ReportLine {
  let body = match format {
    ReportFormat::Human => format_human_line(name, outcome),
    ReportFormat::Tap => format_tap_line(index, name, outcome),
    ReportFormat::Json => serde_json::to_string(&JsonOutcome { name, outcome })
      .unwrap_or_else(|e| format!("{{\"name\":{name:?},\"error\":{:?}}}", e.to_string())),
  };
  ReportLine { body }
}

fn outcome_mark(outcome: &AssayOutcome) -> &'static str {
  match outcome {
    AssayOutcome::Pass => "PASS",
    AssayOutcome::EvalError { .. }
    | AssayOutcome::Recursion
    | AssayOutcome::Timeout
    | AssayOutcome::ResourceLeak => "ERR",
    _ => "FAIL",
  }
}

fn format_human_line(name: &str, outcome: &AssayOutcome) -> String {
  let mut line = format!("{} {name}", outcome_mark(outcome));
  match outcome {
    AssayOutcome::Fail { diff, .. } => {
      let _ = write!(line, "\n  {diff}");
    }
    AssayOutcome::EvalError { message, .. } => {
      let _ = write!(line, "\n  {message}");
    }
    _ => {}
  }
  line
}

fn format_tap_line(index: usize, name: &str, outcome: &AssayOutcome) -> String {
  match outcome {
    AssayOutcome::Pass => format!("ok {index} - {name}"),
    _ => {
      let detail = match outcome {
        AssayOutcome::Fail { diff, .. } => diff.as_str(),
        AssayOutcome::EvalError { message, .. } => message.as_str(),
        AssayOutcome::SnapshotMismatch { diff, .. } => diff.as_str(),
        AssayOutcome::Counterexample { .. } => "counterexample",
        AssayOutcome::Recursion => "infinite recursion",
        AssayOutcome::Timeout => "timeout",
        AssayOutcome::ResourceLeak => "resource leak",
        AssayOutcome::Pass => "",
      };
      if detail.is_empty() {
        format!("not ok {index} - {name}")
      } else {
        format!("not ok {index} - {name}\n  {detail}")
      }
    }
  }
}

fn format_summary(summary: &RunSummary) -> String {
  format!(
    "\n{}/{} passed, {} failed, {} errored",
    summary.passed, summary.total, summary.failed, summary.errored
  )
}

/// The human / TAP lines of `outcomes` (none for JSON, which is one document).
pub fn collect_formatted_lines(
  outcomes: &[(String, AssayOutcome)],
  format: ReportFormat,
) -> Vec<ReportLine> {
  if format == ReportFormat::Json {
    return Vec::new();
  }
  outcomes
    .iter()
    .enumerate()
    .map(|(idx, (name, outcome))| format_line(idx + 1, name, outcome, format))
    .collect()
}

/// The whole report as printed: JSON is a pretty array plus newline; human is one line per
/// case, then a blank line and the summary (no trailing newline); TAP is the plan, then one line
/// per case.
pub fn render_report(
  outcomes: &[(String, AssayOutcome)],
  format: ReportFormat,
  summary: &RunSummary,
) -> Result<String, InfraError> {
  if format == ReportFormat::Json {
    let rows: Vec<JsonOutcome<'_>> = outcomes
      .iter()
      .map(|(name, outcome)| JsonOutcome { name, outcome })
      .collect();
    let json = serde_json::to_string_pretty(&rows)
      .map_err(|e| InfraError::Io(format!("render json report: {e}")))?;
    return Ok(format!("{json}\n"));
  }
  let mut out = String::new();
  if format == ReportFormat::Tap {
    let _ = writeln!(out, "TAP version 13");
    let _ = writeln!(out, "1..{}", summary.total);
  }
  for line in collect_formatted_lines(outcomes, format) {
    let _ = writeln!(out, "{}", line.body);
  }
  if format == ReportFormat::Human {
    out.push_str(&format_summary(summary));
  }
  Ok(out)
}

/// Writes [`render_report`] to `writer`.
pub fn report_outcomes(
  outcomes: &[(String, AssayOutcome)],
  format: ReportFormat,
  summary: &RunSummary,
  writer: &mut dyn Write,
) -> io::Result<()> {
  let text = render_report(outcomes, format, summary).map_err(io::Error::other)?;
  writer.write_all(text.as_bytes())
}

#[cfg(test)]
// Tests parse the rendered JSON back to assert on it.
#[cfg_attr(dylint_lib = "id_effect_lints", allow(schema_parsing_at_io_boundary))]
mod tests {

  use super::*;

  fn sample_outcomes() -> Vec<(String, AssayOutcome)> {
    vec![
      ("passing".into(), AssayOutcome::Pass),
      (
        "failing".into(),
        AssayOutcome::Fail {
          claim: "eq".into(),
          left: None,
          right: None,
          diff: "values differ".into(),
        },
      ),
    ]
  }

  #[test]
  fn report_format_parse() {
    assert_eq!(ReportFormat::parse("human"), Some(ReportFormat::Human));
    assert_eq!(ReportFormat::parse("JSON"), Some(ReportFormat::Json));
    assert_eq!(ReportFormat::parse("tap"), Some(ReportFormat::Tap));
    assert_eq!(ReportFormat::parse("yaml"), None);
  }

  #[test]
  fn human_line_includes_mark_and_diff() {
    let line = format_human_line(
      "case",
      &AssayOutcome::Fail {
        claim: "eq".into(),
        left: None,
        right: None,
        diff: "left != right".into(),
      },
    );
    assert!(line.starts_with("FAIL case"));
    assert!(line.contains("left != right"));
  }

  #[test]
  fn tap_line_ok_and_not_ok() {
    assert_eq!(
      format_tap_line(1, "ok_case", &AssayOutcome::Pass),
      "ok 1 - ok_case"
    );
    let not_ok = format_tap_line(
      2,
      "bad",
      &AssayOutcome::EvalError {
        kind: "throw".into(),
        message: "boom".into(),
        span: None,
      },
    );
    assert!(not_ok.starts_with("not ok 2 - bad"));
    assert!(not_ok.contains("boom"));
    let quiet = format_tap_line(
      3,
      "quiet",
      &AssayOutcome::EvalError {
        kind: "io".into(),
        message: String::new(),
        span: None,
      },
    );
    assert_eq!(quiet, "not ok 3 - quiet");
  }

  #[test]
  fn tap_line_other_outcome_variants() {
    assert!(format_tap_line(1, "r", &AssayOutcome::Recursion).contains("infinite recursion"));
    assert!(format_tap_line(1, "t", &AssayOutcome::Timeout).contains("timeout"));
    assert!(format_tap_line(1, "l", &AssayOutcome::ResourceLeak).contains("resource leak"));
    assert!(
      format_tap_line(
        1,
        "c",
        &AssayOutcome::Counterexample {
          seed: 1,
          shrunk: serde_json::json!(null),
        }
      )
      .contains("counterexample")
    );
    assert!(
      format_tap_line(
        1,
        "s",
        &AssayOutcome::SnapshotMismatch {
          path: "golden.json".into(),
          diff: "mismatch".into(),
        }
      )
      .contains("mismatch")
    );
  }

  #[test]
  fn collect_formatted_lines_human() {
    let outcomes = sample_outcomes();
    let lines = collect_formatted_lines(&outcomes, ReportFormat::Human);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].body.starts_with("PASS"));
    assert!(lines[1].body.starts_with("FAIL"));
  }

  #[test]
  fn report_json_round_trip() {
    let outcomes = sample_outcomes();
    let summary = RunSummary {
      total: 2,
      passed: 1,
      failed: 1,
      errored: 0,
    };
    let mut buf = Vec::new();
    report_outcomes(&outcomes, ReportFormat::Json, &summary, &mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0]["name"], "passing");
  }

  #[test]
  fn report_tap_includes_plan() {
    let outcomes = sample_outcomes();
    let summary = RunSummary {
      total: 2,
      passed: 1,
      failed: 1,
      errored: 0,
    };
    let mut buf = Vec::new();
    report_outcomes(&outcomes, ReportFormat::Tap, &summary, &mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("TAP version 13"));
    assert!(text.contains("1..2"));
    assert!(text.contains("ok 1 - passing"));
    assert!(text.contains("not ok 2 - failing"));
  }

  #[test]
  fn human_report_ends_with_the_summary_and_no_newline() {
    let outcomes = sample_outcomes();
    let summary = RunSummary {
      total: 2,
      passed: 1,
      failed: 1,
      errored: 0,
    };
    let text = render_report(&outcomes, ReportFormat::Human, &summary).unwrap();
    assert!(text.starts_with("PASS passing\nFAIL failing"));
    assert!(text.ends_with("\n\n1/2 passed, 1 failed, 0 errored"));
  }
}
