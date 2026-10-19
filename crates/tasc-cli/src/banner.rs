use std::io::{self, IsTerminal, Write};

const LOGO_LINES: [&str; 5] = [
    "______________________________",
    "___  __/__    |_  ___/_  ____/",
    "__  /  __  /| |____ \\_  /",
    "_  /   _  ___ |___/ // /___",
    "/_/    /_/  |_/____/ \\____/",
];

fn banner(color: bool) -> String {
    let mut output = String::new();
    for (index, line) in LOGO_LINES.iter().enumerate() {
        if color {
            let ansi = ["91", "93", "92", "96", "94"][index % 5];
            output.push_str(&format!("\x1b[1;{ansi}m{line}\x1b[0m\n"));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

pub fn run() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if !stdout.is_terminal() {
        stdout.write_all(b"TASC\n")?;
        stdout.write_all(banner(false).as_bytes())?;
        return stdout.flush();
    }

    let color = std::env::var_os("NO_COLOR").is_none();
    write!(stdout, "\n{}\n", banner(color))?;
    stdout.flush()?
;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::banner;

    #[test]
    fn banner_is_static_and_brand_compatible() {
        let rendered = banner(false);
        assert!(rendered.contains("______________________________"));
        assert!(rendered.contains("___  __/__    |_  ___/_  ____/"));
        assert!(!rendered.contains("\x1b["));
    }
}
