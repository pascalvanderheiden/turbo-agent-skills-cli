use std::io::{self, BufRead, IsTerminal, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Input,
    Tasc,
    Prompt,
    Info,
    Working,
    Warning,
    Error,
    Done,
    Next,
}

impl Event {
    fn style(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::Input => ("›", "Input", "INPUT", "36"),
            Self::Tasc => ("◆", "TASC", "TASC", "35"),
            Self::Prompt => ("?", "Prompt", "PROMPT", "36"),
            Self::Info => ("·", "Info", "INFO", "34"),
            Self::Working => ("...", "Working", "WORKING", "36"),
            Self::Warning => ("!", "Warning", "WARNING", "33"),
            Self::Error => ("×", "Error", "ERROR", "31"),
            Self::Done => ("✓", "Done", "DONE", "32"),
            Self::Next => ("↳", "Next", "NEXT", "2;36"),
        }
    }

    pub fn is_stderr(self) -> bool {
        matches!(
            self,
            Self::Prompt | Self::Working | Self::Warning | Self::Error | Self::Next
        )
    }

    pub fn render(self, body: &str, decoration: bool) -> String {
        let (glyph, label, plain, color) = self.style();
        let prefix = if decoration {
            format!("\x1b[{color}m{glyph} {label}\x1b[0m")
        } else {
            format!("{plain}:")
        };
        format!("{prefix} {}", safe_text(body))
    }

    pub fn decoration(self, json: bool) -> bool {
        let terminal = if self.is_stderr() {
            io::stderr().is_terminal()
        } else {
            io::stdout().is_terminal()
        };
        decoration_enabled(
            terminal,
            std::env::var_os("NO_COLOR").is_some(),
            std::env::var("TERM").as_deref() == Ok("dumb"),
            json,
        )
    }

    pub fn emit(self, body: &str, json: bool) -> io::Result<()> {
        let rendered = self.render(body, self.decoration(json));
        if self.is_stderr() {
            writeln!(io::stderr().lock(), "{rendered}")
        } else {
            writeln!(io::stdout().lock(), "{rendered}")
        }
    }
}

pub fn render_human(
    command: &str,
    schema_version: u32,
    value: &serde_json::Value,
    exit_code: i32,
) -> io::Result<()> {
    if exit_code == 0 {
        Event::Done.emit(&format!("{command} completed"), false)?;
    } else {
        Event::Error.emit(&format!("{command} completed with findings"), false)?;
    }
    Event::Tasc.emit(
        &format!("{command} (schema version {schema_version})"),
        false,
    )?;
    write!(
        io::stdout().lock(),
        "{}",
        render_fields(value, 0, terminal_width())
    )
}

pub fn render_fields(value: &serde_json::Value, indent: usize, width: usize) -> String {
    let mut output = String::new();
    render_value(value, indent, width, &mut output);
    output
}

pub fn render_table(headers: &[String], rows: &[Vec<String>], width: usize) -> String {
    if rows.is_empty() {
        return "(none)\n".to_string();
    }
    let widths = (0..headers.len())
        .map(|column| {
            headers[column].chars().count().max(
                rows.iter()
                    .map(|row| cell_width(&row[column]))
                    .max()
                    .unwrap_or(0),
            )
        })
        .collect::<Vec<_>>();
    let table_width = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 2;
    let mut output = String::new();
    if table_width <= width {
        for (index, header) in headers.iter().enumerate() {
            if index > 0 {
                output.push_str("  ");
            }
            output.push_str(&safe_text(header));
            output.push_str(&" ".repeat(widths[index] - header.chars().count()));
        }
        output.push('\n');
        for (index, header) in headers.iter().enumerate() {
            if index > 0 {
                output.push_str("  ");
            }
            output.push_str(&"-".repeat(widths[index].max(header.chars().count())));
        }
        output.push('\n');
        for row in rows {
            for (index, cell) in row.iter().enumerate() {
                if index > 0 {
                    output.push_str("  ");
                }
                let cell = safe_text(cell);
                output.push_str(&cell);
                if index + 1 < row.len() {
                    output.push_str(&" ".repeat(widths[index] - cell.chars().count()));
                }
            }
            output.push('\n');
        }
    } else {
        for (row_index, row) in rows.iter().enumerate() {
            output.push_str(&format!("- row {}:\n", row_index + 1));
            for (header, value) in headers.iter().zip(row) {
                output.push_str("  ");
                output.push_str(&safe_text(header));
                output.push_str(": ");
                append_indented(&mut output, &safe_text(value), "  ");
            }
        }
    }
    output
}

pub fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|width| width.parse::<usize>().ok())
        .filter(|width| *width > 0)
        .unwrap_or(80)
}

pub fn prompt(prompt: &str, default: Option<&str>, json: bool) -> io::Result<Option<String>> {
    if json || !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Ok(None);
    }
    let default_text = default.map_or_else(String::new, |value| format!(" (default: {value})"));
    let rendered = Event::Prompt.render(
        &format!("{prompt}{default_text}"),
        Event::Prompt.decoration(false),
    );
    write!(io::stderr().lock(), "{rendered} ")?;
    io::stderr().lock().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    let answer = answer.trim().to_string();
    if answer.is_empty() {
        Ok(default.map(str::to_string).or(Some(answer)))
    } else {
        Ok(Some(answer))
    }
}

pub struct WorkingRow {
    active: bool,
    previous_width: usize,
}

impl WorkingRow {
    pub fn start(message: &str, json: bool) -> io::Result<Self> {
        let active = !json && io::stderr().is_terminal();
        if active {
            let rendered = Event::Working.render(message, Event::Working.decoration(false));
            write!(io::stderr().lock(), "\r{rendered}")?;
            io::stderr().lock().flush()?;
            Ok(Self {
                active,
                previous_width: rendered.chars().count(),
            })
        } else {
            Ok(Self {
                active,
                previous_width: 0,
            })
        }
    }

    pub fn finish(&mut self, event: Event, message: &str) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let rendered = event.render(message, event.decoration(false));
        let padding = self.previous_width.saturating_sub(rendered.chars().count());
        let mut stderr = io::stderr().lock();
        write!(stderr, "\r{rendered}{}\n", " ".repeat(padding))?;
        stderr.flush()?;
        self.active = false;
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

fn render_value(value: &serde_json::Value, indent: usize, width: usize, output: &mut String) {
    let prefix = " ".repeat(indent);
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                match value {
                    serde_json::Value::Object(nested) if !nested.is_empty() => {
                        output.push_str(&format!("{prefix}{key}:\n"));
                        render_value(value, indent + 2, width, output);
                    }
                    serde_json::Value::Array(items) => {
                        output.push_str(&format!("{prefix}{key}:\n"));
                        render_array(items, indent + 2, width, output);
                    }
                    _ => {
                        output.push_str(&format!("{prefix}{key}: "));
                        append_indented(output, &scalar(value), &prefix);
                    }
                }
            }
        }
        serde_json::Value::Array(items) => render_array(items, indent, width, output),
        _ => {
            output.push_str(&prefix);
            append_indented(output, &scalar(value), &prefix);
        }
    }
}

fn render_array(items: &[serde_json::Value], indent: usize, width: usize, output: &mut String) {
    if items.is_empty() {
        output.push_str(&format!("{}(none)\n", " ".repeat(indent)));
        return;
    }
    if let Some((headers, rows)) = comparable_rows(items) {
        output.push_str(&indent_text(
            &render_table(&headers, &rows, width.saturating_sub(indent)),
            indent,
        ));
        return;
    }
    for item in items {
        match item {
            serde_json::Value::Object(_) => {
                output.push_str(&format!("{}-:\n", " ".repeat(indent)));
                render_value(item, indent + 2, width, output);
            }
            _ => {
                output.push_str(&format!("{}- ", " ".repeat(indent)));
                append_indented(output, &scalar(item), &" ".repeat(indent + 2));
            }
        }
    }
}

fn comparable_rows(items: &[serde_json::Value]) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let first = items.first()?.as_object()?;
    if first.is_empty()
        || !first.values().all(|value| {
            !matches!(
                value,
                serde_json::Value::Object(_) | serde_json::Value::Array(_)
            )
        })
    {
        return None;
    }
    let headers = first.keys().cloned().collect::<Vec<_>>();
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        let object = item.as_object()?;
        if object.len() != headers.len()
            || headers.iter().any(|header| {
                object.get(header).is_none_or(|value| {
                    matches!(
                        value,
                        serde_json::Value::Object(_) | serde_json::Value::Array(_)
                    )
                })
            })
        {
            return None;
        }
        rows.push(
            headers
                .iter()
                .map(|header| scalar(&object[header]))
                .collect(),
        );
    }
    Some((headers, rows))
}

fn scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "(none)".to_string(),
        serde_json::Value::String(value) => safe_text(value),
        _ => safe_text(&value.to_string()),
    }
}

fn append_indented(output: &mut String, value: &str, continuation_prefix: &str) {
    let mut lines = value.split('\n').peekable();
    while let Some(line) = lines.next() {
        output.push_str(line);
        output.push('\n');
        if lines.peek().is_some() {
            output.push_str(continuation_prefix);
        }
    }
}

fn indent_text(value: &str, indent: usize) -> String {
    let prefix = " ".repeat(indent);
    value
        .lines()
        .map(|line| format!("{prefix}{line}\n"))
        .collect()
}

fn cell_width(value: &str) -> usize {
    value
        .lines()
        .map(str::chars)
        .map(Iterator::count)
        .max()
        .unwrap_or(0)
}

pub fn decoration_enabled(terminal: bool, no_color: bool, dumb: bool, json: bool) -> bool {
    terminal && !no_color && !dumb && !json
}

fn safe_text(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character.is_control() && character != '\n' && character != '\t' {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{decoration_enabled, render_fields, render_table, Event};

    #[test]
    fn vocabulary_colors_only_the_glyph_and_label() {
        for (event, glyph, label, plain, color) in [
            (Event::Input, "›", "Input", "INPUT", "36"),
            (Event::Tasc, "◆", "TASC", "TASC", "35"),
            (Event::Prompt, "?", "Prompt", "PROMPT", "36"),
            (Event::Info, "·", "Info", "INFO", "34"),
            (Event::Working, "...", "Working", "WORKING", "36"),
            (Event::Warning, "!", "Warning", "WARNING", "33"),
            (Event::Error, "×", "Error", "ERROR", "31"),
            (Event::Done, "✓", "Done", "DONE", "32"),
            (Event::Next, "↳", "Next", "NEXT", "2;36"),
        ] {
            assert_eq!(
                event.render("body", true),
                format!("\x1b[{color}m{glyph} {label}\x1b[0m body")
            );
            assert_eq!(event.render("body", false), format!("{plain}: body"));
            assert_eq!(
                event.is_stderr(),
                matches!(
                    event,
                    Event::Prompt | Event::Working | Event::Warning | Event::Error | Event::Next
                )
            );
        }
    }

    #[test]
    fn every_capability_gate_disables_decoration() {
        for terminal in [false, true] {
            for no_color in [false, true] {
                for dumb in [false, true] {
                    for json in [false, true] {
                        assert_eq!(
                            decoration_enabled(terminal, no_color, dumb, json),
                            terminal && !no_color && !dumb && !json
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn body_cannot_inject_color_or_redraw_sequences() {
        assert_eq!(
            Event::Info.render("body\x1b[31m\r", false),
            "INFO: body\\u{1b}[31m\\r"
        );
    }

    #[test]
    fn comparable_rows_align_when_wide_and_stack_without_truncation_when_narrow() {
        let headers = vec!["ID".to_string(), "Path".to_string()];
        let rows = vec![vec![
            "skill:owner/repository".to_string(),
            "/a/long/path/that/must/not/be/truncated".to_string(),
        ]];
        let wide = render_table(&headers, &rows, 100);
        assert!(wide.contains("skill:owner/repository"));
        assert!(wide.contains("/a/long/path/that/must/not/be/truncated"));
        assert!(wide.contains("ID                      Path"));

        let narrow = render_table(&headers, &rows, 20);
        assert!(narrow.contains("ID: skill:owner/repository"));
        assert!(narrow.contains("Path: /a/long/path/that/must/not/be/truncated"));
    }

    #[test]
    fn human_fields_render_comparable_objects_as_a_table() {
        let value = serde_json::json!({
            "items": [
                {"id": "one", "path": "/first"},
                {"id": "two", "path": "/second"}
            ]
        });

        assert_eq!(
            render_fields(&value, 0, 80),
            "items:\n  id   path   \n  ---  -------\n  one  /first\n  two  /second\n"
        );
    }
}
