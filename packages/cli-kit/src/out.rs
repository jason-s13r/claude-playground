//! Where output goes, and in what shape.

use std::io::{self, IsTerminal, Write};

use owo_colors::OwoColorize;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Format {
    #[default]
    Text,
    Json,
}

enum Sink {
    Stdout(io::Stdout),
    Buffer(Vec<u8>),
}

/// The output stream, plus the two facts a renderer needs about it.
pub struct Out {
    sink: Sink,
    format: Format,
    color: bool,
    tty: bool,
}

impl Out {
    /// Standard output. Colour is on only for a terminal that has not been
    /// told otherwise -- `no_color` carries the caller's reading of `NO_COLOR`
    /// and any `--color` flag, since this crate does not read the environment.
    pub fn stdout(format: Format, no_color: bool) -> Out {
        let tty = io::stdout().is_terminal();
        Out {
            sink: Sink::Stdout(io::stdout()),
            format,
            // Colour codes in a JSON document would make it unparseable.
            color: tty && !no_color && format == Format::Text,
            tty,
        }
    }

    /// An in-memory sink, so a renderer can be asserted on without a process.
    pub fn buffer(format: Format) -> Out {
        Out {
            sink: Sink::Buffer(Vec::new()),
            format,
            color: false,
            tty: false,
        }
    }

    /// Force colour on a buffer, to test the coloured path.
    pub fn with_color(mut self, color: bool) -> Out {
        self.color = color;
        self
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn color(&self) -> bool {
        self.color
    }

    pub fn is_tty(&self) -> bool {
        self.tty
    }

    pub fn is_json(&self) -> bool {
        self.format == Format::Json
    }

    /// What was written, for a buffer sink. Empty for stdout.
    pub fn into_string(self) -> String {
        match self.sink {
            Sink::Buffer(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Sink::Stdout(_) => String::new(),
        }
    }

    /// A group or section title.
    pub fn heading(&self, text: &str) -> String {
        if self.color {
            text.cyan().bold().to_string()
        } else {
            text.to_string()
        }
    }

    pub fn good(&self, text: &str) -> String {
        if self.color {
            text.green().to_string()
        } else {
            text.to_string()
        }
    }

    pub fn bad(&self, text: &str) -> String {
        if self.color {
            text.red().to_string()
        } else {
            text.to_string()
        }
    }

    pub fn warn(&self, text: &str) -> String {
        if self.color {
            text.yellow().to_string()
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, text: &str) -> String {
        if self.color {
            text.dimmed().to_string()
        } else {
            text.to_string()
        }
    }
}

impl Write for Out {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.sink {
            Sink::Stdout(s) => s.write(buf),
            Sink::Buffer(b) => b.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.sink {
            Sink::Stdout(s) => s.flush(),
            Sink::Buffer(b) => b.flush(),
        }
    }
}

/// One thing that can be shown to a person or handed to a script.
///
/// `Serialize` is a supertrait so the JSON half comes for free and cannot
/// silently diverge from the type it claims to describe. Override `json` only
/// where the wire shape should differ from the struct.
pub trait View: serde::Serialize {
    fn text(&self, out: &mut Out) -> io::Result<()>;

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// The one place in a program that chooses between the two output shapes.
pub fn emit<V: View + ?Sized>(out: &mut Out, view: &V) -> io::Result<()> {
    match out.format {
        Format::Text => view.text(out),
        Format::Json => {
            let value = view.json();
            let text = serde_json::to_string_pretty(&value)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            writeln!(out, "{text}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct Greeting {
        name: String,
    }

    impl View for Greeting {
        fn text(&self, out: &mut Out) -> io::Result<()> {
            let title = out.heading("Hello");
            writeln!(out, "{title}, {}", self.name)
        }
    }

    #[test]
    fn text_and_json_come_from_the_same_view() {
        let mut out = Out::buffer(Format::Text);
        emit(
            &mut out,
            &Greeting {
                name: "world".into(),
            },
        )
        .unwrap();
        assert_eq!(out.into_string(), "Hello, world\n");

        let mut out = Out::buffer(Format::Json);
        emit(
            &mut out,
            &Greeting {
                name: "world".into(),
            },
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert_eq!(value["name"], "world");
    }

    #[test]
    fn a_buffer_is_uncoloured_unless_asked() {
        let out = Out::buffer(Format::Text);
        assert_eq!(out.heading("Hello"), "Hello");

        let out = Out::buffer(Format::Text).with_color(true);
        assert!(
            out.heading("Hello").contains("\u{1b}["),
            "should carry escapes"
        );
    }

    #[test]
    fn json_output_is_never_coloured() {
        // Escape codes would make the document unparseable, so the flag is
        // refused rather than merely unused.
        let out = Out::stdout(Format::Json, false);
        assert!(!out.color());
    }

    #[test]
    fn json_is_pretty_printed_and_newline_terminated() {
        let mut out = Out::buffer(Format::Json);
        emit(
            &mut out,
            &Greeting {
                name: "world".into(),
            },
        )
        .unwrap();
        let text = out.into_string();
        assert!(text.contains("\n  \"name\""), "pretty printed: {text:?}");
        assert!(text.ends_with('\n'));
    }
}
