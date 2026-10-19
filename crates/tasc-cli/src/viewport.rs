use crate::presentation::{safe_text, Event};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Debug)]
pub enum Key {
    Text(String),
    Paste(String),
    Enter,
    AltEnter,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Debug)]
pub struct Grid {
    pub rows: Vec<String>,
    pub cursor: Option<(usize, usize)>,
}

#[derive(Default)]
pub struct Footer {
    pub state: String,
    pub profile: String,
    pub model: String,
    pub tokens: Option<(i64, i64)>,
}

struct Block {
    event: Option<Event>,
    body: String,
}

pub struct Viewport {
    width: usize,
    height: usize,
    blocks: Vec<Block>,
    working: Option<usize>,
    composer: String,
    cursor: usize,
    scroll: usize,
    new_output: bool,
    pub footer: Footer,
}

impl Viewport {
    pub fn new(width: usize, height: usize, profile: String) -> Self {
        Self {
            width,
            height,
            blocks: Vec::new(),
            working: None,
            composer: String::new(),
            cursor: 0,
            scroll: 0,
            new_output: false,
            footer: Footer {
                state: "Idle".into(),
                profile,
                model: "auto".into(),
                tokens: None,
            },
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
    }

    pub fn content(&mut self, body: &str) {
        self.append(None, body);
    }

    pub fn present(&mut self, event: Event, body: &str) {
        if matches!(event, Event::Done | Event::Error) {
            if let Some(index) = self.working.take() {
                self.blocks[index] = Block {
                    event: Some(event),
                    body: safe_text(body),
                };
                self.new_output |= self.scroll > 0;
                return;
            }
        }
        if event == Event::Working {
            if let Some(index) = self.working {
                self.blocks[index].body = safe_text(body);
                return;
            }
            self.working = Some(self.blocks.len());
        }
        self.append(Some(event), body);
    }

    pub fn settle(&mut self, event: Event, body: &str) {
        if self.working.is_some() {
            self.present(event, body);
        }
    }

    fn append(&mut self, event: Option<Event>, body: &str) {
        // Preserve the visible page when output arrives while auto-follow is paused.
        if self.scroll > 0 {
            self.scroll += block_lines(event, body, self.width.saturating_sub(1)).len();
            self.new_output = true;
        }
        self.blocks.push(Block {
            event,
            body: safe_text(body),
        });
    }

    pub fn key(&mut self, key: Key) -> Option<String> {
        match key {
            Key::Text(text) | Key::Paste(text) => {
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                let text = safe_text(&text);
                self.composer.insert_str(self.cursor, &text);
                self.cursor += text.len();
            }
            Key::AltEnter => {
                self.composer.insert(self.cursor, '\n');
                self.cursor += 1;
            }
            Key::Enter if self.width >= 60 && self.height >= 16 => {
                self.cursor = 0;
                return Some(std::mem::take(&mut self.composer));
            }
            Key::Backspace if self.cursor > 0 => {
                let previous = self.composer[..self.cursor]
                    .char_indices()
                    .last()
                    .unwrap()
                    .0;
                self.composer.drain(previous..self.cursor);
                self.cursor = previous;
            }
            Key::Delete if self.cursor < self.composer.len() => {
                self.composer.remove(self.cursor);
            }
            Key::Left if self.cursor > 0 => {
                self.cursor = self.composer[..self.cursor]
                    .char_indices()
                    .last()
                    .unwrap()
                    .0;
            }
            Key::Right if self.cursor < self.composer.len() => {
                self.cursor += self.composer[self.cursor..]
                    .chars()
                    .next()
                    .unwrap()
                    .len_utf8();
            }
            Key::Home => self.cursor = 0,
            Key::End => {
                self.cursor = self.composer.len();
                self.scroll = 0;
                self.new_output = false;
            }
            Key::PageUp => {
                let transcript_height = self.height.saturating_sub(
                    wrap(&self.composer, self.width.saturating_sub(2).max(1))
                        .len()
                        .clamp(1, 5)
                        + 1,
                );
                let max_scroll = self
                    .blocks
                    .iter()
                    .map(|block| {
                        block_lines(block.event, &block.body, self.width.saturating_sub(1)).len()
                    })
                    .sum::<usize>()
                    .saturating_sub(transcript_height);
                self.scroll = self
                    .scroll
                    .saturating_add(self.height.saturating_sub(7).max(1))
                    .min(max_scroll);
            }
            Key::PageDown => {
                self.scroll = self
                    .scroll
                    .saturating_sub(self.height.saturating_sub(7).max(1));
                if self.scroll == 0 {
                    self.new_output = false;
                }
            }
            _ => {}
        }
        None
    }

    pub fn render(&self) -> Grid {
        let mut rows = vec![" ".repeat(self.width); self.height];
        if self.width < 60 || self.height < 16 {
            if let Some(row) = rows.get_mut(self.height / 2) {
                *row = fit("Resize required: minimum 60x16", self.width);
            }
            return Grid { rows, cursor: None };
        }
        let content_width = self.width - 1;
        let composer_width = self.width - 2;
        let composer = wrap(&self.composer, composer_width);
        let before_cursor = wrap(&self.composer[..self.cursor], composer_width);
        let cursor_row = before_cursor.len() - 1;
        let composer_height = composer.len().clamp(1, 5);
        let composer_start = cursor_row.saturating_sub(composer_height - 1);
        let transcript_height = self.height - composer_height - 1;
        let transcript = self
            .blocks
            .iter()
            .flat_map(|block| block_lines(block.event, &block.body, content_width))
            .collect::<Vec<_>>();
        let max_scroll = transcript.len().saturating_sub(transcript_height);
        let scroll = self.scroll.min(max_scroll);
        let end = transcript.len().saturating_sub(scroll);
        let start = end.saturating_sub(transcript_height);
        for (row, line) in rows.iter_mut().zip(&transcript[start..end]) {
            *row = fit(line, self.width);
        }
        if max_scroll > 0 {
            let position = (max_scroll - scroll) * (transcript_height - 1) / max_scroll;
            for (index, row) in rows[..transcript_height].iter_mut().enumerate() {
                *row = format!(
                    "{}{}",
                    fit(row, content_width),
                    if index == position { "█" } else { "│" }
                );
            }
        }
        for (index, line) in composer
            .iter()
            .skip(composer_start)
            .take(composer_height)
            .enumerate()
        {
            rows[transcript_height + index] = fit(
                &format!("{}{}", if index == 0 { "› " } else { "  " }, line),
                self.width,
            );
        }
        let state = if self.scroll > 0 {
            if self.new_output {
                "Paused · new output"
            } else {
                "Paused"
            }
        } else {
            &self.footer.state
        };
        rows[self.height - 1] = self.footer.render(state, self.width);
        Grid {
            rows,
            cursor: Some((
                2 + UnicodeWidthStr::width(before_cursor.last().unwrap().as_str()),
                transcript_height + cursor_row - composer_start,
            )),
        }
    }
}

impl Footer {
    fn render(&self, state: &str, width: usize) -> String {
        let state = safe_text(state).replace(['\n', '\t'], " ");
        let profile = safe_text(&self.profile).replace(['\n', '\t'], " ");
        let model = safe_text(&self.model).replace(['\n', '\t'], " ");
        let tokens = self
            .tokens
            .map(|(input, output)| input.saturating_add(output));
        let left = format!("{state} · TASC Profile: {profile}");
        let compact_left = format!("{state} · Profile: {profile}");
        let right = format!(
            "Model: {model} · Tokens: {}",
            tokens.map_or_else(|| "unavailable".into(), |tokens| tokens.to_string())
        );
        let compact_right = format!(
            "{model} · {} tok",
            tokens.map_or_else(|| "—".into(), |tokens| tokens.to_string())
        );
        for (left, right) in [
            (left.as_str(), right.as_str()),
            (compact_left.as_str(), compact_right.as_str()),
            (compact_left.as_str(), model.as_str()),
            (compact_left.as_str(), ""),
            (state.as_str(), ""),
        ] {
            let used = UnicodeWidthStr::width(left) + UnicodeWidthStr::width(right);
            if used + usize::from(!right.is_empty()) < width {
                return format!("{left}{}{right}", " ".repeat(width - used));
            }
        }
        fit(&state, width)
    }
}

fn block_lines(event: Option<Event>, body: &str, width: usize) -> Vec<String> {
    let prefix = event.map_or_else(String::new, |event| {
        let (glyph, label, _, _) = event.style();
        format!("{glyph} {label} ")
    });
    let indent = UnicodeWidthStr::width(prefix.as_str());
    wrap(&safe_text(body), width.saturating_sub(indent).max(1))
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "{}{line}",
                if index == 0 {
                    prefix.clone()
                } else {
                    " ".repeat(indent)
                }
            )
        })
        .collect()
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = vec![String::new()];
    let mut column = 0;
    for character in text.chars() {
        if character == '\n' {
            rows.push(String::new());
            column = 0;
            continue;
        }
        let text = if character == '\t' {
            "    ".into()
        } else {
            character.to_string()
        };
        let cells = UnicodeWidthStr::width(text.as_str());
        if column + cells > width {
            rows.push(String::new());
            column = 0;
        }
        rows.last_mut().unwrap().push_str(&text);
        column += cells;
        if column == width {
            rows.push(String::new());
            column = 0;
        }
    }
    rows
}

fn fit(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut cells = 0;
    for character in text.chars() {
        let size = UnicodeWidthChar::width(character).unwrap_or(0);
        if character.is_control() || cells + size > width {
            break;
        }
        result.push(character);
        cells += size;
    }
    result.push_str(&" ".repeat(width - cells));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Viewport {
        Viewport::new(60, 16, "user".into())
    }

    fn assert_grid(grid: &Grid, width: usize, height: usize) {
        assert_eq!(grid.rows.len(), height);
        assert!(grid
            .rows
            .iter()
            .all(|row| UnicodeWidthStr::width(row.as_str()) == width));
    }

    #[test]
    fn fixed_grid_has_transcript_composer_and_one_row_footer() {
        let mut view = viewport();
        view.present(Event::Tasc, "Hello");
        view.key(Key::Text("prompt".into()));
        let grid = view.render();
        assert_grid(&grid, 60, 16);
        assert!(grid.rows[0].starts_with("◆ TASC Hello"));
        assert!(grid.rows[14].starts_with("› prompt"));
        assert!(grid.rows[15].starts_with("Idle · TASC Profile: user"));
        assert_eq!(grid.cursor, Some((8, 14)));
    }

    #[test]
    fn composer_grows_to_five_rows_then_scrolls_without_changing_paste() {
        let mut view = viewport();
        for row in 1..=7 {
            if row > 1 {
                view.key(Key::AltEnter);
            }
            view.key(Key::Text(format!("line {row}")));
            let grid = view.render();
            assert_eq!(grid.cursor.unwrap().1, 14);
            assert!(grid.rows[15 - row.min(5)].starts_with("› "));
        }
        let grid = view.render();
        assert!(grid.rows[10].starts_with("› line 3"));
        assert!(grid.rows[14].starts_with("  line 7"));
        assert_eq!(view.key(Key::Enter).unwrap().lines().count(), 7);
        let paste = "first\nsecond\n\nfourth\n";
        view.key(Key::Paste(paste.into()));
        assert_eq!(view.key(Key::Enter).as_deref(), Some(paste));
        assert_eq!(view.render().cursor, Some((2, 14)));
    }

    #[test]
    fn enter_submits_alt_enter_inserts_and_unicode_editing_is_safe() {
        let mut view = viewport();
        view.key(Key::Text("界é".into()));
        view.key(Key::Left);
        view.key(Key::Delete);
        view.key(Key::AltEnter);
        view.key(Key::Paste("next\r\nlast".into()));
        assert_eq!(view.key(Key::Enter).unwrap(), "界\nnext\nlast");
        view.key(Key::Text("é".into()));
        view.key(Key::Backspace);
        assert_eq!(view.key(Key::Enter).unwrap(), "");
    }

    #[test]
    fn footer_abbreviates_then_hides_tokens_model_and_profile_without_wrapping() {
        let mut view = viewport();
        view.footer.tokens = Some((123, 456));
        view.footer.model = "copilot-model".into();
        view.resize(120, 36);
        let wide = view.render();
        assert!(wide.rows[35].contains("TASC Profile: user"));
        assert!(wide.rows[35].ends_with("Model: copilot-model · Tokens: 579"));
        view.resize(60, 16);
        let narrow = view.render();
        assert!(narrow.rows[15].contains("Profile: user"));
        assert!(narrow.rows[15].ends_with("copilot-model · 579 tok"));
        view.footer.model = "model".repeat(20);
        assert!(!view.render().rows[15].contains("model"));
        view.footer.profile = "profile".repeat(20);
        assert_eq!(view.render().rows[15].trim(), "Idle");
        assert_grid(&view.render(), 60, 16);
    }

    #[test]
    fn minimum_size_and_resizing_preserve_input_and_transcript() {
        let mut view = viewport();
        view.content("banner");
        view.key(Key::Text("draft".into()));
        for (width, height) in [(59, 16), (60, 15), (0, 0), (1, 1)] {
            view.resize(width, height);
            assert_grid(&view.render(), width, height);
            assert_eq!(view.render().cursor, None);
            assert_eq!(view.key(Key::Enter), None);
        }
        view.resize(60, 16);
        assert!(view.render().rows[0].starts_with("banner"));
        assert_eq!(view.key(Key::Enter).unwrap(), "draft");
    }

    #[test]
    fn banner_scrolls_away_and_paused_transcript_does_not_follow_new_output() {
        let mut view = viewport();
        view.content("banner\nstartup summary");
        for index in 0..30 {
            view.present(Event::Info, &format!("row {index}"));
        }
        assert!(!view.render().rows.join("\n").contains("banner"));
        view.key(Key::PageUp);
        let first = view.render().rows[0].clone();
        view.present(Event::Info, "new output");
        assert_eq!(view.render().rows[0], first);
        assert!(view.render().rows[15].contains("Paused · new output"));
        view.key(Key::End);
        assert!(view.render().rows.join("\n").contains("new output"));
        assert!(!view.render().rows[15].contains("Paused"));
    }

    #[test]
    fn scrolling_is_bounded_and_never_pauses_a_short_transcript() {
        let mut view = viewport();
        view.content("short transcript");
        view.key(Key::PageUp);
        assert!(!view.render().rows[15].contains("Paused"));
        for index in 0..30 {
            view.content(&format!("row {index}"));
        }
        for _ in 0..100 {
            view.key(Key::PageUp);
        }
        let scroll = view.scroll;
        view.key(Key::PageUp);
        assert_eq!(view.scroll, scroll);
        view.present(Event::Info, "new output");
        view.key(Key::PageDown);
        assert!(!view.render().rows[0].starts_with("short transcript"));
    }

    #[test]
    fn working_rows_update_and_settle_at_their_original_position() {
        for outcome in [Event::Done, Event::Error] {
            let mut view = viewport();
            view.present(Event::Working, "search");
            view.present(Event::Info, "intervening output");
            view.present(Event::Working, "searching");
            assert_eq!(view.blocks.len(), 2);
            view.present(outcome, "search finished");
            let grid = view.render();
            assert!(grid.rows[0].contains("search finished"));
            assert!(!grid.rows[0].contains("Working"));
            assert!(grid.rows[1].contains("intervening output"));
            assert_eq!(view.blocks.len(), 2);
        }
    }

    #[test]
    fn wide_characters_and_control_sequences_cannot_escape_the_grid() {
        let mut view = viewport();
        view.present(Event::Tasc, &format!("{}\x1b[2J\r", "界é".repeat(100)));
        view.key(Key::Paste("界".repeat(100)));
        view.footer.model = "model\n\t\x1b[2J".into();
        assert_grid(&view.render(), 60, 16);
        assert!(!view.render().rows.join("").contains('\x1b'));
        let (column, row) = view.render().cursor.unwrap();
        assert!(column < 60 && row < 15);
    }
}
