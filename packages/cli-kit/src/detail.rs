//! Label-and-value lines, for the reports that are not tables.
//!
//! `doctor` and `auth status` are the same shape in every tool here: a header
//! of settled facts, then a section per retailer, then a verdict. Each app had
//! written its own `line` and `indented` -- five identical copies, differing
//! only in where they had drifted -- which is the drift this crate exists to
//! prevent. One column width, one indent, one place to change them.
//!
//! Not a table, deliberately. A table draws rules around two columns of
//! wildly different lengths -- a label of six characters beside an absolute
//! path -- and the rules are the least useful thing on the line.

use std::io::{self, Write};

use crate::out::Out;

/// The label column. Wide enough for the longest label any tool uses, so the
/// values line up across sections and across tools.
pub const LABEL: usize = 14;

/// A top-level fact: `config file   /Users/...`.
pub fn field(out: &mut Out, label: &str, value: &str) -> io::Result<()> {
    writeln!(out, "{label:<LABEL$} {value}")
}

/// The same, one level in, under a [`section`].
pub fn indented(out: &mut Out, label: &str, value: &str) -> io::Result<()> {
    writeln!(out, "  {label:<width$} {value}", width = LABEL - 2)
}

/// A blank line and a coloured heading, which is how every section starts.
pub fn section(out: &mut Out, heading: &str) -> io::Result<()> {
    writeln!(out)?;
    let heading = out.heading(heading);
    writeln!(out, "{heading}")
}

/// The last line of a report: `healthy` or `not healthy`.
///
/// Here rather than in each tool because the word is what a person greps for,
/// and two tools disagreeing about it -- "ok" against "healthy" -- is the kind
/// of difference nobody notices until they are comparing two of them.
pub fn verdict(out: &mut Out, healthy: bool) -> io::Result<()> {
    writeln!(out)?;
    let word = if healthy {
        out.good("healthy")
    } else {
        out.bad("not healthy")
    };
    writeln!(out, "{word}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::out::Format;

    #[test]
    fn a_value_starts_at_the_same_column_indented_or_not() {
        // The whole point of sharing this: a nested value that did not line up
        // with the ones above it would read as a different kind of thing.
        let mut out = Out::buffer(Format::Text);
        field(&mut out, "state dir", "/tmp/x").expect("writes");
        indented(&mut out, "store", "291").expect("writes");
        let text = out.into_string();
        let columns: Vec<usize> = text
            .lines()
            .map(|l| l.rfind("  ").map(|i| i + 2).unwrap_or(0))
            .collect();
        assert_eq!(columns[0], columns[1], "{text:?}");
    }

    #[test]
    fn a_verdict_says_one_of_two_words() {
        let mut out = Out::buffer(Format::Text);
        verdict(&mut out, true).expect("writes");
        assert_eq!(out.into_string().trim(), "healthy");

        let mut out = Out::buffer(Format::Text);
        verdict(&mut out, false).expect("writes");
        assert_eq!(out.into_string().trim(), "not healthy");
    }
}
