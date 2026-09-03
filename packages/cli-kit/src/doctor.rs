//! A health report: one pass over a setup, changing none of it.
//!
//! No check aborts the run. A failure early on is exactly when the later lines
//! are most worth seeing, and a report that stops at the first problem makes
//! the user fix things one round trip at a time.

use std::io::{self, Write};

use serde::Serialize;

use crate::out::{Out, View};
use crate::table::table;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    /// Worth saying, but not a failure: the tool still works.
    Warn,
    Fail,
    /// Not applicable here -- an unsupported feature, not a broken one.
    Skip,
}

impl Status {
    pub fn symbol(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Fail => "fail",
            Status::Skip => "n/a",
        }
    }

    fn paint(self, out: &Out) -> String {
        let text = self.symbol();
        match self {
            Status::Ok => out.good(text),
            Status::Warn => out.warn(text),
            Status::Fail => out.bad(text),
            Status::Skip => out.dim(text),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Check {
    pub fn ok(name: impl Into<String>, detail: impl Into<String>) -> Check {
        Check::new(name, Status::Ok, detail)
    }

    pub fn warn(name: impl Into<String>, detail: impl Into<String>) -> Check {
        Check::new(name, Status::Warn, detail)
    }

    pub fn fail(name: impl Into<String>, detail: impl Into<String>) -> Check {
        Check::new(name, Status::Fail, detail)
    }

    pub fn skip(name: impl Into<String>, detail: impl Into<String>) -> Check {
        Check::new(name, Status::Skip, detail)
    }

    pub fn new(name: impl Into<String>, status: Status, detail: impl Into<String>) -> Check {
        Check {
            name: name.into(),
            status,
            detail: detail.into(),
            hint: None,
        }
    }

    /// What to do about it. Worth its own field: the detail says what is wrong,
    /// the hint says what to type.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Check {
        self.hint = Some(hint.into());
        self
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn new() -> Report {
        Report::default()
    }

    pub fn push(&mut self, check: Check) -> &mut Report {
        self.checks.push(check);
        self
    }

    /// A warning is not a failure. Only `Fail` gates the exit status, so a
    /// missing optional setting does not break a script that did not need it.
    pub fn healthy(&self) -> bool {
        !self.checks.iter().any(|c| c.status == Status::Fail)
    }
}

impl View for Report {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let mut t = table(&["", "check", "detail"]);
        for check in &self.checks {
            let detail = match &check.hint {
                Some(hint) => format!("{}\n{}", check.detail, out.dim(hint)),
                None => check.detail.clone(),
            };
            t.add_row(vec![check.status.paint(out), check.name.clone(), detail]);
        }
        writeln!(out, "{t}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::out::{emit, Format};

    #[test]
    fn a_warning_does_not_make_a_report_unhealthy() {
        let mut report = Report::new();
        report.push(Check::ok("build", "0.1.0"));
        report.push(Check::warn("store", "none selected"));
        assert!(report.healthy());

        report.push(Check::fail("account", "session expired"));
        assert!(!report.healthy());
    }

    #[test]
    fn an_empty_report_is_healthy() {
        assert!(Report::new().healthy());
    }

    #[test]
    fn text_shows_the_status_the_name_and_the_hint() {
        let mut report = Report::new();
        report.push(Check::fail("account", "not signed in").with_hint("run: gsnz auth login"));
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &report).unwrap();
        let text = out.into_string();
        assert!(text.contains("fail"));
        assert!(text.contains("account"));
        assert!(text.contains("gsnz auth login"));
    }

    #[test]
    fn json_carries_the_status_as_a_word_and_omits_an_absent_hint() {
        let mut report = Report::new();
        report.push(Check::ok("build", "0.1.0"));
        let mut out = Out::buffer(Format::Json);
        emit(&mut out, &report).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert_eq!(value["checks"][0]["status"], "ok");
        assert!(value["checks"][0].get("hint").is_none());
    }
}
