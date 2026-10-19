use std::io::{self, Write};

const LOGO_LINES: [&str; 5] = [
    "______________________________",
    "___  __/__    |_  ___/_  ____/",
    "__  /  __  /| |____ \\_  /",
    "_  /   _  ___ |___/ // /___",
    "/_/    /_/  |_/____/ \\____/",
];

pub(crate) fn render(color: bool) -> String {
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
    let color = tasc::presentation::Event::Tasc.decoration(false);
    stdout.write_all(render(color).as_bytes())?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn plain_banner_is_one_static_ascii_render() {
        assert_eq!(
            render(false),
            concat!(
                "______________________________\n",
                "___  __/__    |_  ___/_  ____/\n",
                "__  /  __  /| |____ \\_  /\n",
                "_  /   _  ___ |___/ // /___\n",
                "/_/    /_/  |_/____/ \\____/\n",
            )
        );
    }

    #[test]
    fn decorated_banner_only_adds_color_without_redraw() {
        let mut rendered = render(true);
        for color in ["91", "93", "92", "96", "94"] {
            let escape = format!("\x1b[1;{color}m");
            assert_eq!(rendered.matches(&escape).count(), 1);
            rendered = rendered.replace(&escape, "");
        }
        assert_eq!(rendered.matches("\x1b[0m").count(), 5);
        assert_eq!(rendered.replace("\x1b[0m", ""), render(false));
    }
}
