use std::io::{self, IsTerminal, Write};

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
            Self::Prompt | Self::Working | Self::Warning | Self::Error
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
    use super::{decoration_enabled, Event};

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
                    Event::Prompt | Event::Working | Event::Warning | Event::Error
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
}
