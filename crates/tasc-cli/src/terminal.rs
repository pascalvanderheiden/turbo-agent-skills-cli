use crate::{
    presentation::Event,
    viewport::{Key, Viewport},
};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as TerminalEvent, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
    },
    execute, queue,
    style::Print,
    terminal::{self, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, IsTerminal, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::mpsc::{self, UnboundedReceiver};

static ACTIVE: Mutex<Option<Arc<Mutex<Viewport>>>> = Mutex::new(None);

pub fn enabled(tty: bool, json: bool, no_color: bool, dumb: bool) -> bool {
    tty && !json && !no_color && !dumb
}

pub fn update(change: impl FnOnce(&mut Viewport)) -> Option<io::Result<()>> {
    let active = ACTIVE.lock().ok()?.clone()?;
    Some(change_and_draw(&active, change))
}

pub fn emit(event: Event, body: &str) -> Option<io::Result<()>> {
    update(|view| view.present(event, body))
}

pub fn content(body: &str) -> Option<io::Result<()>> {
    update(|view| view.content(body))
}

fn change_and_draw(view: &Mutex<Viewport>, change: impl FnOnce(&mut Viewport)) -> io::Result<()> {
    let mut view = view
        .lock()
        .map_err(|_| io::Error::other("viewport unavailable"))?;
    change(&mut view);
    let grid = view.render();
    let mut stdout = io::stdout().lock();
    queue!(stdout, Hide)?;
    for (row, text) in grid.rows.iter().enumerate() {
        queue!(stdout, MoveTo(0, row as u16))?;
        queue!(stdout, Print(styled_row(text, row + 1 == grid.rows.len())))?;
    }
    if let Some((column, row)) = grid.cursor {
        queue!(stdout, MoveTo(column as u16, row as u16), Show)?;
    }
    stdout.flush()
}

fn styled_row(text: &str, status_line: bool) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }
    if status_line {
        return format!("\x1b[90m{text}\x1b[0m");
    }
    if text.starts_with("› ") {
        return format!("\x1b[36m{text}\x1b[0m");
    }
    let event_color = [
        Event::Input,
        Event::Tasc,
        Event::Prompt,
        Event::Info,
        Event::Working,
        Event::Warning,
        Event::Error,
        Event::Done,
        Event::Next,
    ]
    .into_iter()
    .find_map(|event| {
        let (glyph, label, _, color) = event.style();
        let prefix = format!("{glyph} {label}");
        text.starts_with(&prefix).then_some(color)
    });
    event_color.map_or_else(
        || text.to_string(),
        |color| format!("\x1b[{color}m{text}\x1b[0m"),
    )
}

pub struct ManagedTerminal {
    input: UnboundedReceiver<io::Result<Option<String>>>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl ManagedTerminal {
    pub fn open(json: bool, profile: String) -> io::Result<Option<Self>> {
        let tty =
            io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal();
        if !enabled(
            tty,
            json,
            std::env::var_os("NO_COLOR").is_some(),
            std::env::var("TERM").as_deref() == Ok("dumb"),
        ) {
            return Ok(None);
        }
        let (width, height) = terminal::size()?;
        let view = Arc::new(Mutex::new(Viewport::new(
            width as usize,
            height as usize,
            profile,
        )));
        let (sender, input) = mpsc::unbounded_channel();
        let stop = Arc::new(AtomicBool::new(false));
        terminal::enable_raw_mode()?;
        let mut guard = Self {
            input,
            stop: stop.clone(),
            reader: None,
        };
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            DisableLineWrap,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        *ACTIVE
            .lock()
            .map_err(|_| io::Error::other("viewport unavailable"))? = Some(view.clone());
        change_and_draw(&view, |_| {})?;
        guard.reader = Some(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let result = (|| {
                    if !event::poll(Duration::from_millis(50))? {
                        return Ok(None);
                    }
                    let event = event::read()?;
                    let mut submitted = None;
                    change_and_draw(&view, |view| match event {
                        TerminalEvent::Resize(width, height) => {
                            view.resize(width as usize, height as usize)
                        }
                        TerminalEvent::Paste(text) => {
                            view.key(Key::Paste(text));
                        }
                        TerminalEvent::Mouse(mouse) => match mouse.kind {
                            MouseEventKind::ScrollUp => {
                                view.key(Key::PageUp);
                            }
                            MouseEventKind::ScrollDown => {
                                view.key(Key::PageDown);
                            }
                            _ => {}
                        },
                        TerminalEvent::Key(key) if key.kind != KeyEventKind::Release => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                match key.code {
                                    KeyCode::Char('c') => {
                                        submitted = Some(Err(io::Error::new(
                                            io::ErrorKind::Interrupted,
                                            "cancelled",
                                        )))
                                    }
                                    KeyCode::Char('d') => submitted = Some(Ok(None)),
                                    _ => {}
                                }
                                return;
                            }
                            let key = match key.code {
                                KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                                    Key::AltEnter
                                }
                                KeyCode::Enter => Key::Enter,
                                KeyCode::Char(character) => Key::Text(character.to_string()),
                                KeyCode::Backspace => Key::Backspace,
                                KeyCode::Delete => Key::Delete,
                                KeyCode::Left => Key::Left,
                                KeyCode::Right => Key::Right,
                                KeyCode::Home => Key::Home,
                                KeyCode::End => Key::End,
                                KeyCode::Up => Key::Up,
                                KeyCode::Down => Key::Down,
                                KeyCode::Esc => Key::Escape,
                                KeyCode::PageUp => Key::PageUp,
                                KeyCode::PageDown => Key::PageDown,
                                _ => return,
                            };
                            if let Some(line) = view.key(key) {
                                submitted = Some(Ok(Some(line)));
                            }
                        }
                        _ => {}
                    })?;
                    Ok(submitted)
                })();
                match result {
                    Ok(Some(message)) => {
                        if sender.send(message).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        }));
        Ok(Some(guard))
    }

    pub async fn read_line(&mut self, line: &mut String) -> io::Result<usize> {
        match self.input.recv().await.transpose()? {
            Some(Some(input)) => {
                line.push_str(&input);
                line.push('\n');
                Ok(input.len() + 1)
            }
            _ => Ok(0),
        }
    }
}

impl Drop for ManagedTerminal {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Ok(mut active) = ACTIVE.lock() {
            *active = None;
        }
        let _ = execute!(
            io::stdout(),
            Show,
            DisableMouseCapture,
            DisableBracketedPaste,
            EnableLineWrap,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::{enabled, styled_row};

    #[test]
    fn streaming_paths_never_activate_terminal_management() {
        for tty in [true, false] {
            for json in [true, false] {
                for no_color in [true, false] {
                    for dumb in [true, false] {
                        assert_eq!(
                            enabled(tty, json, no_color, dumb),
                            tty && !json && !no_color && !dumb
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn ordinary_terminal_rows_are_not_colored() {
        assert_eq!(styled_row("Command Palette", false), "Command Palette");
        assert_eq!(styled_row("   ", false), "   ");
    }

    #[test]
    fn input_output_errors_and_status_use_stable_colors() {
        assert_eq!(
            styled_row("› command", false),
            "\u{1b}[36m› command\u{1b}[0m"
        );
        assert_eq!(
            styled_row("◆ TASC result", false),
            "\u{1b}[35m◆ TASC result\u{1b}[0m"
        );
        assert_eq!(
            styled_row("× Error failed", false),
            "\u{1b}[31m× Error failed\u{1b}[0m"
        );
        assert_eq!(
            styled_row("Idle · Profile: user", true),
            "\u{1b}[90mIdle · Profile: user\u{1b}[0m"
        );
    }
}
