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
const RAINBOW: [&str; 6] = ["91", "93", "92", "96", "94", "95"];

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
        queue!(stdout, Print(rainbow_row(text, row)))?;
    }
    if let Some((column, row)) = grid.cursor {
        queue!(stdout, MoveTo(column as u16, row as u16), Show)?;
    }
    stdout.flush()
}

fn rainbow_row(text: &str, row: usize) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }
    let body_color = RAINBOW[row % RAINBOW.len()];
    let event_style = [
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
        text.starts_with(&prefix).then_some((prefix.len(), color))
    });
    match event_style {
        Some((length, event_color)) => format!(
            "\x1b[1;{event_color}m{}\x1b[0m\x1b[{body_color}m{}\x1b[0m",
            &text[..length],
            &text[length..]
        ),
        None => format!("\x1b[{body_color}m{text}\x1b[0m"),
    }
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
    use super::{enabled, rainbow_row, RAINBOW};

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
    fn managed_terminal_rows_cycle_through_the_rainbow() {
        for (row, color) in RAINBOW.iter().enumerate() {
            assert_eq!(
                rainbow_row("Command Palette", row),
                format!("\u{1b}[{color}mCommand Palette\u{1b}[0m")
            );
        }
        assert_eq!(rainbow_row("   ", 0), "   ");
    }

    #[test]
    fn event_labels_keep_semantic_emphasis_over_rainbow_bodies() {
        let rendered = rainbow_row("× Error failed", 4);
        assert!(rendered.starts_with("\u{1b}[1;31m× Error\u{1b}[0m"));
        assert!(rendered.contains("\u{1b}[94m failed\u{1b}[0m"));
    }
}
