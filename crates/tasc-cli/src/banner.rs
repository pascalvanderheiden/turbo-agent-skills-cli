use std::{
    io::{self, IsTerminal, Write},
    thread,
    time::Duration,
};

const LOGO_LINES: [&str; 6] = [
    "  _________   _____ ______",
    " /_  __/   | / ___// ____/",
    "  / / / /| | \\__ \\/ /",
    " / / / ___ |___/ / /___",
    "/_/ /_/  |_/____/\\____/",
    "   TURBO AGENT SKILLS CLI",
];
const COLORS: [&str; 6] = ["91", "93", "92", "96", "94", "95"];
const FRAME_COUNT: usize = 32;
const FRAME_DELAY: Duration = Duration::from_millis(75);

struct CursorGuard;

impl CursorGuard {
    fn hide(stdout: &mut impl Write) -> io::Result<Self> {
        write!(stdout, "\x1b[?25l")?;
        stdout.flush()?;
        Ok(Self)
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout().lock();
        let _ = write!(stdout, "\x1b[0m\x1b[?25h\n");
        let _ = stdout.flush();
    }
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

fn frame(index: usize, color: bool) -> String {
    let cycle = index % 16;
    let position = if cycle < 8 { cycle } else { 15 - cycle };
    let motion = " ".repeat(position);
    let mut output = format!("{motion}━━━━⚡\n\n");

    for (line_index, line) in LOGO_LINES.iter().enumerate() {
        if color {
            output.push_str(&format!("\x1b[1;{}m{line}\x1b[0m\n", COLORS[line_index]));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    output.push_str("      FAST • LOCAL • SAFE\n");
    output
}

pub fn run() -> io::Result<()> {
    let color = color_enabled();
    let mut stdout = io::stdout().lock();

    if !stdout.is_terminal() {
        stdout.write_all(frame(FRAME_COUNT - 1, false).as_bytes())?;
        return stdout.flush();
    }

    let _cursor = CursorGuard::hide(&mut stdout)?;
    for index in 0..FRAME_COUNT {
        write!(stdout, "\x1b[H\x1b[J{}", frame(index, color))?;
        stdout.flush()?;
        thread::sleep(FRAME_DELAY);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::frame;

    #[test]
    fn frame_contains_speed_theme_and_brand() {
        let rendered = frame(0, false);

        assert!(rendered.contains("⚡"));
        assert!(!rendered.contains("🚀"));
        assert!(rendered.contains("TURBO AGENT SKILLS CLI"));
        assert!(rendered.contains("FAST • LOCAL • SAFE"));
        assert!(!rendered.contains("\x1b["));
    }

    #[test]
    fn frame_moves_the_speed_streak() {
        let first = frame(0, false);
        let second = frame(4, false);

        assert_ne!(first, second);
    }
}
