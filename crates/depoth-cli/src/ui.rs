//! The one decision, made once.
//!
//! A command-line tool talks to two audiences that want opposite things. A
//! person at a terminal wants colour, alignment, a spinner that stays on one
//! line, and a prompt when something is missing. A pipe, a log file and CI
//! want bytes that do not move and do not carry escape sequences.
//!
//! Every tool that scatters `if is_terminal` through its commands ends up with
//! one command that forgot. So the question is asked exactly once, in
//! [`Face::decide`], and the answer is carried by a [`Ui`] that every command
//! writes through. Nothing in `commands/` calls `println!`.
//!
//! The mechanism is deliberately dull: **the renderers always emit ANSI**, and
//! [`anstream::AutoStream`] strips it on the way out when the face is plain.
//! One code path produces both transcripts, so the plain one cannot drift from
//! the coloured one, and a test can assert the exact bytes by constructing a
//! [`Ui::capture`].
//!
//! Colour vocabulary, and the whole of it:
//!
//! | Style | Means | Used for |
//! |---|---|---|
//! | [`ACCENT`] | the reader acts on this | paths, URLs, the thing just made |
//! | [`DIM`] | secondary | labels, tree rules, column headers |
//! | [`OK`] / [`WARN`] / [`BAD`] | state | a result, never a noun |
//!
//! One accent and one dim, plus three states. A second accent would mean the
//! first one stopped meaning anything.

use std::cell::RefCell;
use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::rc::Rc;
use std::time::Duration;

use anstream::{AutoStream, ColorChoice};
use anstyle::{AnsiColor, Color, Style};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use unicode_width::UnicodeWidthStr;

/// The one accent. Sky, the same hue the site's own theme makes its accent,
/// as the nearest ANSI slot rather than a truecolour triple -- a 256-colour
/// terminal and a light background both have to stay legible, and the user's
/// own palette is better tuned to their screen than ours is.
pub const ACCENT: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
/// Secondary text. Never carries meaning on its own.
pub const DIM: Style = Style::new().dimmed();
/// Emphasis without colour, so it survives a monochrome terminal.
pub const STRONG: Style = Style::new().bold();
pub const OK: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
pub const WARN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
pub const BAD: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
/// No style. Spelled out so a call site never has to think about it.
pub const PLAIN: Style = Style::new();

/// Wraps `text` in `style`'s escape sequences.
///
/// Always, regardless of the face: the stream strips them if it must. Width is
/// never measured on the result -- see [`Cell`].
pub fn paint(style: Style, text: impl Display) -> String {
    let text = text.to_string();
    // Painting nothing produces nothing. Without this an empty cell is two
    // escape sequences, which is invisible but is not whitespace, so the
    // trailing-space trim in `Table::render` would step over it.
    if text.is_empty() {
        return text;
    }
    format!("{style}{text}{}", style.render_reset())
}

/// Who is on the other end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// A person is watching. Colour, a live spinner, and prompts when an
    /// answer is missing.
    Terminal,
    /// A pipe, a file, CI, `NO_COLOR`, `--no-color` or `--quiet`. Plain text,
    /// no animation, and a missing answer is an error rather than a question.
    Plain,
}

impl Face {
    /// The decision. Everything else in this crate reads the result.
    ///
    /// The order is the order of least surprise: an explicit flag outranks an
    /// environment variable, which outranks what the file descriptor looks
    /// like.
    pub fn decide(no_color: bool, quiet: bool) -> Self {
        // `--quiet` is listed here and not only on the progress code because a
        // person who asked for less does not want a spinner redrawing either.
        if no_color || quiet {
            return Face::Plain;
        }
        // https://no-color.org: set, to any value, means do not colour.
        if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return Face::Plain;
        }
        // The matching opt-in, for someone piping into a pager that renders
        // ANSI. Checked before the terminal test, which is the whole point.
        if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty() && v != "0") {
            return Face::Terminal;
        }
        // Stdout, not stderr: a command's output is the thing being captured,
        // and a split decision would colour half a transcript.
        if std::io::stdout().is_terminal() {
            Face::Terminal
        } else {
            Face::Plain
        }
    }

    fn choice(self) -> ColorChoice {
        match self {
            Face::Terminal => ColorChoice::Always,
            Face::Plain => ColorChoice::Never,
        }
    }
}

type Sink = AutoStream<Box<dyn Write>>;

/// An in-memory transcript, for tests.
///
/// Test-only, and the reason the look is covered rather than hoped for: a
/// `Ui` built on one of these produces the exact bytes a pipe would receive.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Default)]
pub struct Transcript(Rc<RefCell<Vec<u8>>>);

#[cfg_attr(not(test), allow(dead_code))]
impl Transcript {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).into_owned()
    }
}

impl Write for Transcript {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Everything the tool says, and the only thing that says it.
pub struct Ui {
    face: Face,
    quiet: bool,
    out: RefCell<Sink>,
    err: RefCell<Sink>,
}

impl Ui {
    /// The real streams.
    pub fn new(face: Face, quiet: bool) -> Self {
        let choice = face.choice();
        Self {
            face,
            quiet,
            out: RefCell::new(AutoStream::new(
                Box::new(std::io::stdout()) as Box<dyn Write>,
                choice,
            )),
            err: RefCell::new(AutoStream::new(
                Box::new(std::io::stderr()) as Box<dyn Write>,
                choice,
            )),
        }
    }

    /// A `Ui` writing into memory. Both streams share one buffer, so a test
    /// reads the transcript in the order a human would have seen it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn capture(face: Face, quiet: bool) -> (Self, Transcript) {
        let transcript = Transcript::default();
        let choice = face.choice();
        let sink = || {
            RefCell::new(AutoStream::new(
                Box::new(transcript.clone()) as Box<dyn Write>,
                choice,
            ))
        };
        let ui = Self {
            face,
            quiet,
            out: sink(),
            err: sink(),
        };
        (ui, transcript)
    }

    pub fn face(&self) -> Face {
        self.face
    }

    /// Whether a prompt may be shown. False means every answer has to arrive
    /// as a flag, and a missing one is an error naming that flag.
    pub fn interactive(&self) -> bool {
        self.face == Face::Terminal
    }

    // -- the raw writes -----------------------------------------------------

    /// A line of the command's actual output. Stdout, always printed.
    pub fn say(&self, text: impl Display) {
        let mut out = self.out.borrow_mut();
        let _ = writeln!(out, "{text}");
        let _ = out.flush();
    }

    /// A line of commentary: what is happening, not what was asked for.
    /// Stderr, so it never lands in a redirect, and silent under `--quiet`.
    pub fn note(&self, text: impl Display) {
        if self.quiet {
            return;
        }
        let mut err = self.err.borrow_mut();
        let _ = writeln!(err, "{text}");
        let _ = err.flush();
    }

    /// A blank line of commentary. Collapses under `--quiet` with the rest.
    pub fn gap(&self) {
        self.note("");
    }

    // -- the vocabulary -----------------------------------------------------

    /// The one-line result of a command. `✔` plus a sentence, the operative
    /// noun in the accent.
    pub fn done(&self, text: impl Display) {
        self.note(format!("{} {text}", paint(OK, "✔")));
    }

    /// Something to act on that is not fatal.
    pub fn warn(&self, text: impl Display) {
        self.note(format!("{} {text}", paint(WARN, "!")));
    }

    /// A fatal message in the one-line form. A diagnostic with a location
    /// belongs in [`Diagnostic`] instead; this is for "cargo is not on PATH".
    pub fn fail(&self, text: impl Display) {
        let mut err = self.err.borrow_mut();
        let _ = writeln!(err, "{} {text}", paint(BAD, "✖"));
        let _ = err.flush();
    }

    /// A command's heading: its name, and what it is operating on.
    pub fn heading(&self, name: &str, subject: impl Display) {
        self.note(format!(
            "{} {}",
            paint(STRONG, format!("depoth {name}")),
            paint(DIM, subject)
        ));
        self.gap();
    }

    /// Aligned columns, to stdout. See [`Table`].
    pub fn table(&self, table: &Table) {
        for line in table.render() {
            self.say(line);
        }
    }

    /// Aligned columns, to stderr -- for a summary block, which is commentary
    /// on the work rather than the work's output.
    pub fn table_note(&self, table: &Table) {
        for line in table.render() {
            self.note(line);
        }
    }

    /// Output produced by a subprocess, forwarded unchanged.
    ///
    /// Cargo's own diagnostics come through here. They carry ANSI only when
    /// the face is a terminal, because the face is what set `CARGO_TERM_COLOR`
    /// on the child -- and the stream would strip them anyway if it were not.
    pub fn verbatim(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut err = self.err.borrow_mut();
        let _ = err.write_all(text.as_bytes());
        if !text.ends_with('\n') {
            let _ = writeln!(err);
        }
        let _ = err.flush();
    }

    /// A diagnostic in rustc's shape. Stderr, and never suppressed: `--quiet`
    /// means "only the findings", not "no findings".
    pub fn diagnostic(&self, d: &Diagnostic) {
        let mut err = self.err.borrow_mut();
        // A blank line after each, as rustc does: two diagnostics run together
        // read as one with a confusing second half.
        let _ = write!(err, "{}", d.render());
        let _ = writeln!(err);
        let _ = err.flush();
    }

    /// A run of phases, reported as they finish.
    ///
    /// `labels` is every label the caller will use, so the column can be
    /// measured before the first one is printed. On a terminal the current
    /// phase is a spinner that overwrites itself; everywhere else the same
    /// lines appear with no animation and no cursor movement.
    pub fn steps(&self, labels: &[&str]) -> Steps<'_> {
        let width = labels.iter().copied().map(width_of).max().unwrap_or(0);
        let bar = (self.face == Face::Terminal && !self.quiet).then(|| {
            let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
            bar.set_style(
                ProgressStyle::with_template("  {spinner:.cyan} {wide_msg}")
                    .expect("static template")
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
            );
            bar.enable_steady_tick(Duration::from_millis(80));
            bar
        });
        Steps {
            ui: self,
            bar,
            width,
        }
    }
}

/// A run of phases. Created by [`Ui::steps`].
pub struct Steps<'a> {
    ui: &'a Ui,
    bar: Option<ProgressBar>,
    width: usize,
}

impl Steps<'_> {
    /// Names the phase now running. Visible only on a terminal; elsewhere the
    /// record printed by [`Steps::done`] is the whole report, because a log
    /// does not want both "starting" and "finished".
    pub fn begin(&self, label: &str) {
        if let Some(bar) = &self.bar {
            bar.set_message(label.to_string());
        }
    }

    /// Records a finished phase. One permanent line, columns aligned.
    pub fn done(&self, label: &str, detail: impl Display) {
        let pad = " ".repeat(self.width.saturating_sub(width_of(label)));
        let line = format!(
            "  {} {}{pad}  {}",
            paint(OK, "✔"),
            paint(PLAIN, label),
            paint(DIM, detail)
        );
        self.print(line);
    }

    /// A determinate bar over `len` units of work, for a phase whose size is
    /// known. Hidden off a terminal, where it would be a thousand lines.
    pub fn bar(&self, len: u64, label: &str) -> ProgressBar {
        if self.bar.is_none() {
            return ProgressBar::hidden();
        }
        // The outer spinner and an inner bar on one line fight each other.
        if let Some(bar) = &self.bar {
            bar.set_draw_target(ProgressDrawTarget::hidden());
        }
        let bar = ProgressBar::with_draw_target(Some(len), ProgressDrawTarget::stderr());
        bar.set_style(
            ProgressStyle::with_template("  {msg} {bar:24.cyan/blue} {pos}/{len}")
                .expect("static template")
                .progress_chars("━━─"),
        );
        bar.set_message(label.to_string());
        bar
    }

    /// Returns the spinner to the foreground after a [`Steps::bar`] finishes.
    pub fn resume(&self) {
        if let Some(bar) = &self.bar {
            bar.set_draw_target(ProgressDrawTarget::stderr());
        }
    }

    /// Prints above the spinner rather than through it.
    fn print(&self, line: String) {
        match &self.bar {
            Some(bar) => bar.suspend(|| self.ui.note(line)),
            None => self.ui.note(line),
        }
    }

    /// Clears the spinner. Nothing after this writes to the phase area.
    pub fn end(self) {
        if let Some(bar) = &self.bar {
            bar.finish_and_clear();
        }
    }
}

// -- columns ----------------------------------------------------------------

/// Display width, which is not byte length and not `char` count.
///
/// `{:<20}` pads by `char` count, so a CJK title -- two columns per character
/// -- pushes every following column right by its own length. Measuring here is
/// the difference between a grid and a suggestion of one.
pub fn width_of(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Right,
}

/// One cell, as styled runs. Width is the sum of the runs' *text* widths; the
/// escape sequences are added after padding, never measured.
#[derive(Debug, Clone)]
pub struct Cell {
    parts: Vec<(String, Style)>,
    align: Align,
}

impl Cell {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            parts: vec![(text.into(), PLAIN)],
            align: Align::Left,
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            parts: vec![(text.into(), style)],
            align: Align::Left,
        }
    }

    /// Appends a run in another style -- a dim tree rule before an accented
    /// label, say, which has to be one cell so that one width is measured.
    pub fn then(mut self, text: impl Into<String>, style: Style) -> Self {
        self.parts.push((text.into(), style));
        self
    }

    pub fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }

    pub fn width(&self) -> usize {
        self.parts.iter().map(|(t, _)| width_of(t)).sum()
    }

    fn render(&self, to: usize) -> String {
        let body: String = self
            .parts
            .iter()
            .map(|(t, s)| paint(*s, t))
            .collect::<Vec<_>>()
            .concat();
        let pad = " ".repeat(to.saturating_sub(self.width()));
        match self.align {
            Align::Left => format!("{body}{pad}"),
            Align::Right => format!("{pad}{body}"),
        }
    }
}

/// Rows of cells, rendered into a grid whose columns are as wide as their
/// widest cell and no wider.
#[derive(Debug, Default)]
pub struct Table {
    rows: Vec<Vec<Cell>>,
    indent: usize,
    gap: usize,
}

impl Table {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            indent: 2,
            gap: 2,
        }
    }

    pub fn indent(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }

    pub fn row(&mut self, cells: Vec<Cell>) -> &mut Self {
        self.rows.push(cells);
        self
    }

    pub fn render(&self) -> Vec<String> {
        let columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0usize; columns];
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.width());
            }
        }
        let pad = " ".repeat(self.indent);
        let gap = " ".repeat(self.gap);
        self.rows
            .iter()
            .map(|row| {
                let last = row.len().saturating_sub(1);
                let cells: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        // A left-aligned final column needs no padding, and
                        // trailing spaces are invisible on a terminal and
                        // noise in a diff. A right-aligned one still does:
                        // its padding is on the left, where it is the
                        // alignment.
                        let to = if i == last && c.align == Align::Left {
                            0
                        } else {
                            widths[i]
                        };
                        c.render(to)
                    })
                    .collect();
                format!("{pad}{}", cells.join(&gap)).trim_end().to_string()
            })
            .collect()
    }
}

// -- diagnostics ------------------------------------------------------------

/// rustc's shape, because it is the shape every Rust developer already reads
/// and because `error[code]` makes a finding greppable.
///
/// `annotate-snippets` is the library for this and is already sanctioned by
/// the plan, but almost every finding here is about a *route* rather than a
/// byte range, and the subset that has a real span -- a config key, a
/// frontmatter field -- needs the header, the arrow and a caret run. That is
/// the fifty lines below rather than a dependency and a conversion layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
    /// The colour a finding of this severity is counted in.
    pub fn style(self) -> Style {
        match self {
            Severity::Error => BAD.bold(),
            Severity::Warning => WARN.bold(),
        }
    }
}

/// The source line a diagnostic points at.
#[derive(Debug, Clone)]
pub struct Snippet {
    pub line: usize,
    pub column: usize,
    pub text: String,
    /// How many columns the caret run covers. Zero means one caret.
    pub span: usize,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The rule that failed, e.g. `dead-link`. Becomes `error[dead-link]`.
    pub code: Option<String>,
    pub message: String,
    /// A file path, a `path:line:col`, or a route. Whatever names the thing.
    pub location: Option<String>,
    pub snippet: Option<Snippet>,
    /// `("help", ...)` or `("note", ...)`, rendered as rustc's `= help:`.
    pub notes: Vec<(&'static str, String)>,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: Some(code.into()),
            message: message.into(),
            location: None,
            snippet: None,
            notes: Vec::new(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(code, message)
        }
    }

    pub fn at(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    pub fn snippet(mut self, snippet: Snippet) -> Self {
        self.snippet = Some(snippet);
        self
    }

    pub fn note(mut self, text: impl Into<String>) -> Self {
        self.notes.push(("note", text.into()));
        self
    }

    pub fn help(mut self, text: impl Into<String>) -> Self {
        self.notes.push(("help", text.into()));
        self
    }

    /// The whole diagnostic, newline-terminated.
    pub fn render(&self) -> String {
        let severity = self.severity;
        let head = match &self.code {
            Some(code) => format!("{}[{code}]", severity.label()),
            None => severity.label().to_string(),
        };
        let mut out = format!(
            "{}{} {}\n",
            paint(severity.style(), head),
            paint(STRONG, ":"),
            paint(STRONG, &self.message)
        );

        // The gutter is as wide as the largest line number, which is rustc's
        // rule and the reason its output stays aligned as a file grows.
        let gutter = self
            .snippet
            .as_ref()
            .map(|s| s.line.to_string().len())
            .unwrap_or(1);
        let pad = " ".repeat(gutter);
        let rule = paint(DIM, "|");

        if let Some(location) = &self.location {
            out.push_str(&format!(
                "{pad}{} {}\n",
                paint(DIM, "-->"),
                paint(PLAIN, location)
            ));
        }
        if let Some(snippet) = &self.snippet {
            let column = snippet.column.max(1);
            let carets = "^".repeat(snippet.span.max(1));
            // The caret offset is a display width too: a tab or a wide
            // character before the span moves the caret.
            let before: String = snippet.text.chars().take(column - 1).collect();
            out.push_str(&format!("{pad} {rule}\n"));
            out.push_str(&format!(
                "{} {rule} {}\n",
                paint(DIM, snippet.line),
                snippet.text
            ));
            out.push_str(&format!(
                "{pad} {rule} {}{}\n",
                " ".repeat(width_of(&before)),
                paint(severity.style(), carets)
            ));
        }
        for (kind, text) in &self.notes {
            out.push_str(&format!(
                "{pad} {} {} {text}\n",
                paint(DIM, "="),
                paint(STRONG, format!("{kind}:"))
            ));
        }
        out
    }
}

// -- formatting -------------------------------------------------------------

/// `248913` -> `248,913`. A byte count is read, not computed, so it is grouped.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `248913` -> `243.1 KiB`. The aside beside the exact count, never instead.
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// `0.52s`, `1m 03s`. Milliseconds below a second, because the whole point of
/// the two-clock design is that one of the clocks is short.
pub fn elapsed(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 1.0 {
        format!("{}ms", d.as_millis())
    } else if secs < 60.0 {
        format!("{secs:.2}s")
    } else {
        format!("{}m {:02}s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_face_emits_no_escape_sequences() {
        let (ui, transcript) = Ui::capture(Face::Plain, false);
        ui.done(format!("built {}", paint(ACCENT, "dist")));
        ui.warn("one page has no title");
        assert_eq!(
            transcript.text(),
            "✔ built dist\n! one page has no title\n",
            "the plain face must be the same text with the colour removed"
        );
    }

    #[test]
    fn a_terminal_face_emits_the_same_text_with_escape_sequences() {
        let (ui, transcript) = Ui::capture(Face::Terminal, false);
        ui.done("built dist");
        let text = transcript.text();
        assert!(text.contains("\u{1b}["), "expected ANSI, got {text:?}");
        // Strip and compare: one renderer, two transcripts.
        let stripped = anstream::adapter::strip_str(&text).to_string();
        assert_eq!(stripped, "✔ built dist\n");
    }

    #[test]
    fn quiet_keeps_the_output_and_drops_the_commentary() {
        let (ui, transcript) = Ui::capture(Face::Plain, true);
        ui.note("compiling templates");
        ui.done("built dist");
        ui.say("/docs/install");
        ui.diagnostic(&Diagnostic::error("dead-link", "nope"));
        assert_eq!(
            transcript.text(),
            // The blank line after a diagnostic is rustc's, and it is part of
            // the diagnostic rather than commentary, so --quiet keeps it.
            "/docs/install\nerror[dead-link]: nope\n\n",
            "--quiet is 'only what was asked for and anything wrong'"
        );
    }

    #[test]
    fn columns_are_measured_by_display_width_not_char_count() {
        // Two CJK characters are two chars and four columns. `{:<4}` would pad
        // them by two more and push the next column right.
        let mut table = Table::new().indent(0);
        table.row(vec![Cell::new("日本"), Cell::new("x")]);
        table.row(vec![Cell::new("abcd"), Cell::new("y")]);
        assert_eq!(table.render(), ["日本  x", "abcd  y"]);
    }

    #[test]
    fn a_styled_run_does_not_widen_its_column() {
        let mut table = Table::new().indent(0);
        table.row(vec![Cell::styled("ab", ACCENT), Cell::new("x")]);
        table.row(vec![Cell::new("abcd"), Cell::new("y")]);
        let lines: Vec<String> = table
            .render()
            .iter()
            .map(|l| anstream::adapter::strip_str(l).to_string())
            .collect();
        assert_eq!(lines, ["ab    x", "abcd  y"]);
    }

    #[test]
    fn a_right_aligned_column_puts_the_units_under_each_other() {
        let mut table = Table::new().indent(2);
        table.row(vec![Cell::new("pages"), Cell::new("12").right()]);
        table.row(vec![Cell::new("bytes"), Cell::new("248,913").right()]);
        assert_eq!(table.render(), ["  pages       12", "  bytes  248,913"]);
    }

    #[test]
    fn the_last_column_is_never_padded() {
        let mut table = Table::new().indent(0);
        table.row(vec![Cell::new("a"), Cell::new("long value")]);
        table.row(vec![Cell::new("b"), Cell::new("x")]);
        assert_eq!(table.render(), ["a  long value", "b  x"]);
    }

    #[test]
    fn a_diagnostic_without_a_span_names_the_route() {
        let d = Diagnostic::error("dead-link", "\"../setup\" was not built")
            .at("/docs/install")
            .note("dist/docs/install/index.html");
        assert_eq!(
            anstream::adapter::strip_str(&d.render()).to_string(),
            concat!(
                "error[dead-link]: \"../setup\" was not built\n",
                " --> /docs/install\n",
                "  = note: dist/docs/install/index.html\n",
            )
        );
    }

    #[test]
    fn a_diagnostic_with_a_span_is_rustc_shaped() {
        let d = Diagnostic::error("frontmatter", "unknown field `titel`")
            .at("content/docs/install.md:2:1")
            .snippet(Snippet {
                line: 2,
                column: 1,
                text: "titel: Install".into(),
                span: 5,
            })
            .help("expected `title`");
        assert_eq!(
            anstream::adapter::strip_str(&d.render()).to_string(),
            concat!(
                "error[frontmatter]: unknown field `titel`\n",
                " --> content/docs/install.md:2:1\n",
                "  |\n",
                "2 | titel: Install\n",
                "  | ^^^^^\n",
                "  = help: expected `title`\n",
            )
        );
    }

    #[test]
    fn the_caret_lands_under_a_wide_character() {
        let d = Diagnostic::warning("x", "y").snippet(Snippet {
            line: 10,
            column: 3,
            text: "日本語".into(),
            span: 2,
        });
        let text = anstream::adapter::strip_str(&d.render()).to_string();
        // Two wide characters precede column 3, so four spaces, not two.
        assert!(text.contains("   |     ^^\n"), "{text:?}");
    }

    #[test]
    fn numbers_are_grouped_and_scaled() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(248_913), "248,913");
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(248_913), "243.1 KiB");
    }

    #[test]
    fn elapsed_switches_units_rather_than_printing_zero() {
        assert_eq!(elapsed(Duration::from_millis(160)), "160ms");
        assert_eq!(elapsed(Duration::from_millis(1840)), "1.84s");
        assert_eq!(elapsed(Duration::from_secs(63)), "1m 03s");
    }

    #[test]
    fn no_color_in_the_environment_outranks_a_terminal() {
        // The flag path is pure and testable; the env path is read once here
        // and is the reason `decide` takes no `&self`.
        assert_eq!(Face::decide(true, false), Face::Plain);
        assert_eq!(Face::decide(false, true), Face::Plain);
    }
}
