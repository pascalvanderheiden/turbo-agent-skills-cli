use crate::{
    commands::{
        alias_target, available_during_operation, canonical_command_input, is_complete_command,
        ranked_commands, CommandSafety, SLASH_COMMANDS,
    },
    presentation::{safe_text, Event},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
    Up,
    Down,
    Escape,
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
    palette: Option<Palette>,
    picker: Option<Picker>,
    active_operation: bool,
    pub footer: Footer,
}

const PICKER_MAX_ROWS: usize = 8;

struct PickerItem {
    value: String,
    session: bool,
    profile: bool,
    label: String,
}

enum PickerKind {
    Model,
    Scope(String),
    Inventory { command: String, multi: bool },
}

struct PickerLevel {
    kind: PickerKind,
    items: Vec<PickerItem>,
    filter: String,
    selected: usize,
    status: Option<String>,
    chosen: Vec<String>,
}

struct Picker {
    original_composer: String,
    original_cursor: usize,
    levels: Vec<PickerLevel>,
}

impl PickerLevel {
    fn matches(&self) -> Vec<&PickerItem> {
        let filter = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                item.value.to_lowercase().contains(&filter)
                    || item.label.to_lowercase().contains(&filter)
            })
            .collect()
    }
}

struct Palette {
    original_composer: String,
    original_cursor: usize,
    selected: Option<usize>,
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
            palette: None,
            picker: None,
            active_operation: false,
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
        self.clamp_scroll();
    }

    pub fn set_operation_active(&mut self, active: bool) {
        self.active_operation = active;
        self.refresh_palette_selection();
    }

    pub fn open_palette_browse(&mut self) {
        let original_composer = self
            .palette
            .as_ref()
            .map(|palette| palette.original_composer.clone())
            .unwrap_or_else(|| self.composer.clone());
        let original_cursor = self
            .palette
            .as_ref()
            .map(|palette| palette.original_cursor)
            .unwrap_or(self.cursor);
        let prefix = self
            .composer
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(self.composer.as_str(), |(index, _)| &self.composer[..index]);
        self.composer = format!("{prefix}/");
        self.cursor = self.composer.len();
        self.palette = Some(Palette {
            original_composer,
            original_cursor,
            selected: None,
        });
        self.refresh_palette_selection();
    }

    pub fn open_model_picker(&mut self) {
        self.palette = None;
        let (original_composer, original_cursor) = self
            .picker
            .take()
            .map(|picker| (picker.original_composer, picker.original_cursor))
            .unwrap_or_else(|| (self.composer.clone(), self.cursor));
        self.picker = Some(Picker {
            original_composer,
            original_cursor,
            levels: vec![PickerLevel {
                kind: PickerKind::Model,
                items: Vec::new(),
                filter: String::new(),
                selected: 0,
                status: Some("Loading models…".into()),
                chosen: Vec::new(),
            }],
        });
    }

    /// Opens an Inventory Projection Picker over `(stable id, label)` entries.
    /// Completing it only yields a slash command line; mutating commands still
    /// go through the plan and approval flow.
    pub fn open_inventory_picker(
        &mut self,
        command: &str,
        multi: bool,
        entries: &[(String, String)],
    ) {
        self.palette = None;
        let (original_composer, original_cursor) = self
            .picker
            .take()
            .map(|picker| (picker.original_composer, picker.original_cursor))
            .unwrap_or_else(|| (self.composer.clone(), self.cursor));
        self.picker = Some(Picker {
            original_composer,
            original_cursor,
            levels: vec![PickerLevel {
                kind: PickerKind::Inventory {
                    command: command.trim_start_matches('/').to_string(),
                    multi,
                },
                items: entries
                    .iter()
                    .map(|(id, label)| PickerItem {
                        value: id.clone(),
                        session: false,
                        profile: false,
                        label: label.clone(),
                    })
                    .collect(),
                filter: String::new(),
                selected: 0,
                status: None,
                chosen: Vec::new(),
            }],
        });
    }

    pub fn set_model_picker_models(
        &mut self,
        models: &[String],
        session: &str,
        profile: Option<&str>,
    ) {
        if let Some(level) = self.picker_loading_level() {
            level.status = None;
            level.items = std::iter::once("auto")
                .chain(models.iter().map(String::as_str).filter(|id| *id != "auto"))
                .map(|id| PickerItem {
                    value: id.to_string(),
                    session: id == session,
                    profile: Some(id) == profile,
                    label: String::new(),
                })
                .collect();
        }
    }

    pub fn fail_model_picker(&mut self, message: &str) {
        if let Some(level) = self.picker_loading_level() {
            level.status = Some(format!("Models unavailable: {}", safe_text(message)));
        }
    }

    fn picker_loading_level(&mut self) -> Option<&mut PickerLevel> {
        self.picker
            .as_mut()
            .and_then(|picker| picker.levels.last_mut())
            .filter(|level| matches!(level.kind, PickerKind::Model) && level.items.is_empty())
    }

    pub fn picker_open(&self) -> bool {
        self.picker.is_some()
    }

    pub fn content(&mut self, body: &str) {
        self.append(None, body);
    }

    pub fn present(&mut self, event: Event, body: &str) {
        if matches!(event, Event::Done | Event::Error) {
            if let Some(index) = self.working.take() {
                self.replace(index, event, body);
                return;
            }
        }
        if event == Event::Working {
            if let Some(index) = self.working {
                self.replace(index, event, body);
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

    fn replace(&mut self, index: usize, event: Event, body: &str) {
        let width = self.width.saturating_sub(1);
        if self.scroll > 0 {
            let lengths = self
                .blocks
                .iter()
                .map(|block| block_lines(block.event, &block.body, width).len())
                .collect::<Vec<_>>();
            let visible_start = lengths
                .iter()
                .sum::<usize>()
                .saturating_sub(self.scroll)
                .saturating_sub(self.transcript_height());
            let block_start = lengths[..index].iter().sum::<usize>();
            if block_start >= visible_start {
                self.scroll = self
                    .scroll
                    .saturating_add(block_lines(Some(event), body, width).len())
                    .saturating_sub(lengths[index]);
            }
            self.new_output = true;
        }
        self.blocks[index] = Block {
            event: Some(event),
            body: safe_text(body),
        };
        self.clamp_scroll();
    }

    fn transcript_height(&self) -> usize {
        let composer_height = wrap(&self.composer, self.width.saturating_sub(2).max(1))
            .len()
            .clamp(1, 5);
        self.height
            .saturating_sub(composer_height + 1 + self.overlay_rows(composer_height).len())
    }

    fn max_scroll(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| block_lines(block.event, &block.body, self.width.saturating_sub(1)).len())
            .sum::<usize>()
            .saturating_sub(self.transcript_height())
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
        if self.scroll == 0 {
            self.new_output = false;
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
        if self.picker.is_some() {
            return self.picker_key(key);
        }
        let previous_composer = self.composer.clone();
        let previous_cursor = self.cursor;
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
                if self.palette.is_some() {
                    return self.select_palette_command();
                }
                return self.submit_composer();
            }
            Key::Backspace if self.cursor > 0 => {
                let previous = self.composer[..self.cursor]
                    .grapheme_indices(true)
                    .last()
                    .unwrap()
                    .0;
                self.composer.drain(previous..self.cursor);
                self.cursor = previous;
            }
            Key::Delete if self.cursor < self.composer.len() => {
                let length = self.composer[self.cursor..]
                    .graphemes(true)
                    .next()
                    .unwrap()
                    .len();
                self.composer.drain(self.cursor..self.cursor + length);
            }
            Key::Left if self.cursor > 0 => {
                self.cursor = self.composer[..self.cursor]
                    .grapheme_indices(true)
                    .last()
                    .unwrap()
                    .0;
            }
            Key::Right if self.cursor < self.composer.len() => {
                self.cursor += self.composer[self.cursor..]
                    .graphemes(true)
                    .next()
                    .unwrap()
                    .len();
            }
            Key::Home => self.cursor = 0,
            Key::End => {
                self.cursor = self.composer.len();
                self.scroll = 0;
                self.new_output = false;
            }
            Key::Up if self.palette.is_some() => self.move_palette_selection(false),
            Key::Down if self.palette.is_some() => self.move_palette_selection(true),
            Key::Escape if self.palette.is_some() => self.escape_palette(),
            Key::PageUp => {
                self.scroll = self
                    .scroll
                    .saturating_add(self.transcript_height().max(1))
                    .min(self.max_scroll());
            }
            Key::PageDown => {
                self.scroll = self.scroll.saturating_sub(self.transcript_height().max(1));
            }
            _ => {}
        }
        if self.palette.is_none() {
            if self.palette_query().is_some() {
                self.palette = Some(Palette {
                    original_composer: previous_composer,
                    original_cursor: previous_cursor,
                    selected: None,
                });
                self.refresh_palette_selection();
            }
        } else if self.palette_query().is_none() {
            self.palette = None;
        } else {
            self.refresh_palette_selection();
        }
        self.clamp_scroll();
        None
    }

    fn submit_composer(&mut self) -> Option<String> {
        self.cursor = 0;
        self.palette = None;
        let input = std::mem::take(&mut self.composer);
        self.clamp_scroll();
        Some(input)
    }

    fn select_palette_command(&mut self) -> Option<String> {
        let input = self.composer.trim();
        let canonical_input = canonical_command_input(input);
        if is_complete_command(input) {
            if self.active_operation && !available_during_operation(&canonical_input) {
                return None;
            }
            return self.submit_palette_input(canonical_input);
        }
        if input
            .strip_prefix('/')
            .and_then(|command| command.split_once(char::is_whitespace))
            .is_some_and(|(_, arguments)| !arguments.trim().is_empty())
        {
            return None;
        }
        let Some(index) = self.selected_palette_command() else {
            return None;
        };
        let command = &SLASH_COMMANDS[index];
        if command.name == "help" {
            self.open_palette_browse();
            return None;
        }
        let prefix = self
            .composer
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(self.composer.as_str(), |(index, _)| &self.composer[..index]);
        let canonical = canonical_command_input(&format!("/{0}", command.syntax));
        if command.required_arguments {
            self.composer = format!("{prefix}/{0} ", command.name);
            self.cursor = self.composer.len();
            self.palette = None;
            self.clamp_scroll();
            None
        } else {
            self.submit_palette_input(canonical)
        }
    }

    fn submit_palette_input(&mut self, input: String) -> Option<String> {
        self.palette = None;
        self.composer.clear();
        self.cursor = 0;
        self.clamp_scroll();
        Some(input)
    }

    fn escape_palette(&mut self) {
        let query = self.palette_query().unwrap_or_default().trim();
        if !query.is_empty() {
            if let Some(palette) = self.palette.as_ref() {
                self.composer = palette.original_composer.clone();
                self.composer.insert(palette.original_cursor, '/');
                self.cursor = palette.original_cursor + 1;
            }
            if let Some(palette) = self.palette.as_mut() {
                palette.selected = None;
            }
            self.refresh_palette_selection();
            return;
        }
        if let Some(palette) = self.palette.take() {
            self.composer = palette.original_composer;
            self.cursor = palette.original_cursor.min(self.composer.len());
        }
        self.clamp_scroll();
    }

    fn palette_query(&self) -> Option<&str> {
        let (index, character) = self
            .composer
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())?;
        if character == '/' {
            Some(&self.composer[index + 1..])
        } else {
            None
        }
    }

    fn palette_matches(&self) -> Vec<usize> {
        ranked_commands(
            self.palette_query().unwrap_or_default(),
            self.active_operation,
        )
    }

    fn palette_enabled(&self, index: usize) -> bool {
        !self.active_operation || SLASH_COMMANDS[index].active_allowed
    }

    fn selected_palette_command(&self) -> Option<usize> {
        let matches = self.palette_matches();
        self.palette
            .as_ref()
            .and_then(|palette| palette.selected)
            .filter(|index| matches.contains(index) && self.palette_enabled(*index))
            .or_else(|| {
                matches
                    .into_iter()
                    .find(|index| self.palette_enabled(*index))
            })
    }

    fn refresh_palette_selection(&mut self) {
        let matches = self.palette_matches();
        let current = self
            .palette
            .as_ref()
            .and_then(|palette| palette.selected)
            .filter(|index| matches.contains(index) && self.palette_enabled(*index));
        let selected = current.or_else(|| {
            matches
                .into_iter()
                .find(|index| self.palette_enabled(*index))
        });
        if let Some(palette) = self.palette.as_mut() {
            palette.selected = selected;
        }
    }

    fn move_palette_selection(&mut self, down: bool) {
        let choices = self
            .palette_matches()
            .into_iter()
            .filter(|index| self.palette_enabled(*index))
            .collect::<Vec<_>>();
        if choices.is_empty() {
            if let Some(palette) = self.palette.as_mut() {
                palette.selected = None;
            }
            return;
        }
        let selected = self.selected_palette_command();
        let current = selected
            .and_then(|selected| choices.iter().position(|index| *index == selected))
            .unwrap_or(0);
        let next = if down {
            (current + 1) % choices.len()
        } else {
            (current + choices.len() - 1) % choices.len()
        };
        if let Some(palette) = self.palette.as_mut() {
            palette.selected = Some(choices[next]);
        }
    }

    fn composer_height(&self) -> usize {
        wrap(&self.composer, self.width.saturating_sub(2).max(1))
            .len()
            .clamp(1, 5)
    }

    fn picker_capacity(&self) -> usize {
        self.height
            .saturating_sub(self.composer_height() + 2)
            .saturating_sub(2)
            .clamp(1, PICKER_MAX_ROWS)
    }

    fn picker_key(&mut self, key: Key) -> Option<String> {
        let capacity = self.picker_capacity();
        let enabled = self.width >= 60 && self.height >= 16;
        let picker = self.picker.as_mut()?;
        let level = picker.levels.last_mut()?;
        let count = level.matches().len();
        let multi = matches!(level.kind, PickerKind::Inventory { multi: true, .. });
        match key {
            Key::Text(text) if multi && text == " " => {
                if count > 0 {
                    let id = level.matches()[level.selected.min(count - 1)].value.clone();
                    match level.chosen.iter().position(|chosen| *chosen == id) {
                        Some(position) => {
                            level.chosen.remove(position);
                        }
                        None => level.chosen.push(id),
                    }
                }
                level.status = None;
            }
            Key::Delete if multi => level.chosen.clear(),
            Key::Enter if enabled && multi => {
                if level.chosen.is_empty() {
                    level.status = Some("Select at least one with Space".into());
                } else if let PickerKind::Inventory { command, .. } = &level.kind {
                    let ordered = level
                        .items
                        .iter()
                        .filter(|item| level.chosen.contains(&item.value))
                        .map(|item| item.value.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let line = format!("/{command} {ordered}");
                    if let Some(picker) = self.picker.take() {
                        self.composer = picker.original_composer;
                        self.cursor = picker.original_cursor.min(self.composer.len());
                    }
                    self.clamp_scroll();
                    return Some(line);
                }
            }
            Key::Text(text) | Key::Paste(text) => {
                let text = safe_text(&text).replace('\n', "");
                level.filter.push_str(&text);
                level.selected = 0;
            }
            Key::Backspace => {
                level.filter.pop();
                level.selected = 0;
            }
            Key::Up if count > 0 => level.selected = (level.selected + count - 1) % count,
            Key::Down if count > 0 => level.selected = (level.selected + 1) % count,
            Key::PageUp => level.selected = level.selected.saturating_sub(capacity),
            Key::PageDown if count > 0 => {
                level.selected = (level.selected + capacity).min(count - 1)
            }
            Key::Escape => {
                if !level.filter.is_empty() {
                    level.filter.clear();
                    level.selected = 0;
                } else if picker.levels.len() > 1 {
                    picker.levels.pop();
                } else if let Some(picker) = self.picker.take() {
                    self.composer = picker.original_composer;
                    self.cursor = picker.original_cursor.min(self.composer.len());
                    self.clamp_scroll();
                }
            }
            Key::Enter if enabled && count > 0 => {
                let value = level.matches()[level.selected.min(count - 1)].value.clone();
                match &level.kind {
                    PickerKind::Model => {
                        picker.levels.push(PickerLevel {
                            kind: PickerKind::Scope(value),
                            items: ["session", "profile"]
                                .into_iter()
                                .map(|scope| PickerItem {
                                    value: scope.into(),
                                    session: false,
                                    profile: false,
                                    label: String::new(),
                                })
                                .collect(),
                            filter: String::new(),
                            selected: 0,
                            status: None,
                            chosen: Vec::new(),
                        });
                    }
                    PickerKind::Inventory { command, .. } => {
                        let line = format!("/{command} {value}");
                        if let Some(picker) = self.picker.take() {
                            self.composer = picker.original_composer;
                            self.cursor = picker.original_cursor.min(self.composer.len());
                        }
                        self.clamp_scroll();
                        return Some(line);
                    }
                    PickerKind::Scope(model) => {
                        let command = if value == "profile" {
                            format!("/model profile {model}")
                        } else {
                            format!("/model {model}")
                        };
                        if let Some(picker) = self.picker.take() {
                            self.composer = picker.original_composer;
                            self.cursor = picker.original_cursor.min(self.composer.len());
                        }
                        self.clamp_scroll();
                        return Some(command);
                    }
                }
            }
            _ => {}
        }
        self.clamp_scroll();
        None
    }

    fn overlay_rows(&self, composer_height: usize) -> Vec<String> {
        if self.picker.is_some() {
            self.picker_rows(composer_height)
        } else {
            self.palette_rows(composer_height)
        }
    }

    fn picker_rows(&self, composer_height: usize) -> Vec<String> {
        let Some(level) = self.picker.as_ref().and_then(|picker| picker.levels.last()) else {
            return Vec::new();
        };
        let available = self.height.saturating_sub(composer_height + 2);
        let title = match &level.kind {
            PickerKind::Model => "Model Picker".to_string(),
            PickerKind::Scope(model) => format!("Scope Picker · {}", safe_text(model)),
            PickerKind::Inventory { command, .. } => {
                format!("Inventory Picker · /{}", safe_text(command))
            }
        };
        let multi = matches!(level.kind, PickerKind::Inventory { multi: true, .. });
        let header = if level.filter.is_empty() {
            format!("{title} · type to filter")
        } else {
            format!("{title} · filter: {}", safe_text(&level.filter))
        };
        let mut rows = vec![header];
        let matches = level.matches();
        if let Some(status) = level
            .status
            .as_ref()
            .filter(|_| !multi || matches.is_empty())
        {
            rows.push(status.clone());
        } else if matches.is_empty() {
            rows.push("No matches · Esc clears the filter".into());
        } else {
            let selected = level.selected.min(matches.len() - 1);
            let visible = available
                .saturating_sub(2)
                .clamp(1, PICKER_MAX_ROWS)
                .min(matches.len());
            let start = selected
                .saturating_sub(visible - 1)
                .min(matches.len() - visible);
            for (offset, item) in matches.iter().skip(start).take(visible).enumerate() {
                let marker = if start + offset == selected {
                    "›"
                } else {
                    " "
                };
                let label = match (&level.kind, item.value.as_str()) {
                    (PickerKind::Scope(_), "session") => "This Agent Session".to_string(),
                    (PickerKind::Scope(_), "profile") => "Profile default".to_string(),
                    _ => item.value.clone(),
                };
                let mut row = if multi {
                    let check = if level.chosen.contains(&item.value) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    format!("{marker} {check} {}", safe_text(&label))
                } else {
                    format!("{marker} {}", safe_text(&label))
                };
                if matches!(level.kind, PickerKind::Inventory { .. }) && !item.label.is_empty() {
                    row = format!("{row} · {}", safe_text(&item.label));
                }
                if item.session {
                    row.push_str(" · active session");
                }
                if item.profile {
                    row.push_str(" · profile default");
                }
                rows.push(row);
            }
            if multi {
                if let Some(status) = &level.status {
                    rows.push(status.clone());
                }
                rows.push(format!(
                    "Showing {}-{} of {} · {} selected · Space toggle · Enter confirm · Del clear · Esc back",
                    start + 1,
                    start + visible,
                    matches.len(),
                    level.chosen.len()
                ));
            } else if matches!(level.kind, PickerKind::Inventory { .. }) {
                rows.push(format!(
                    "Showing {}-{} of {} · Enter select · Esc back",
                    start + 1,
                    start + visible,
                    matches.len()
                ));
            } else {
                rows.push(format!(
                    "Selection {}/{} · Enter select · Esc back",
                    selected + 1,
                    matches.len()
                ));
            }
        }
        rows.truncate(available);
        rows
    }

    fn palette_rows(&self, composer_height: usize) -> Vec<String> {
        if self.palette.is_none() {
            return Vec::new();
        }
        let available = self.height.saturating_sub(composer_height + 2);
        if available == 0 {
            return Vec::new();
        }
        let matches = self.palette_matches();
        let selected = self.selected_palette_command();
        let query = self.palette_query().unwrap_or_default().trim();
        let header = alias_target(query).map_or_else(
            || {
                if query.is_empty() {
                    "Command Palette · Browse all commands".into()
                } else {
                    format!("Command Palette · /{query}")
                }
            },
            |canonical| format!("Command Palette · /{query} → /{canonical}"),
        );
        let details = selected.map(|index| {
            let command = &SLASH_COMMANDS[index];
            let arguments = if command.arguments.is_empty() {
                "none"
            } else {
                command.arguments
            };
            let safety = match command.safety {
                CommandSafety::ReadOnly => "Read-only",
                CommandSafety::PlanAndApproval => "Plan + approval",
            };
            (
                command,
                arguments.to_string(),
                format!(
                    "{safety} · {}",
                    if self.active_operation {
                        "Allowed now"
                    } else {
                        "Available when idle"
                    }
                ),
            )
        });
        let details_rows = if details.is_some() { 4 } else { 0 };
        let list_room = available.saturating_sub(1 + details_rows);
        let mut rows = vec![header];
        if matches.is_empty() {
            rows.push("No matching Slash Commands · input remains editable".into());
        } else {
            let selected_position =
                selected.and_then(|selected| matches.iter().position(|index| *index == selected));
            let visible = list_room.min(8).min(matches.len());
            let start = selected_position
                .map(|position| position.saturating_sub(visible.saturating_sub(1)))
                .unwrap_or(0)
                .min(matches.len().saturating_sub(visible));
            for index in matches.iter().skip(start).take(visible) {
                let command = &SLASH_COMMANDS[*index];
                let marker = if Some(*index) == selected { "›" } else { " " };
                let availability = if self.active_operation && !command.active_allowed {
                    " · Available when idle"
                } else {
                    ""
                };
                rows.push(format!("{marker} /{}{}", command.syntax, availability));
            }
        }
        if let Some((command, arguments, safety)) = details {
            rows.push(command.description.into());
            rows.push(format!("Arguments: {arguments}"));
            rows.push(format!(
                "Selection {}/{}",
                selected
                    .and_then(|index| matches.iter().position(|candidate| *candidate == index))
                    .map_or(0, |position| position + 1),
                matches.len(),
            ));
            rows.push(safety);
        }
        rows.truncate(available);
        rows
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
        let palette = self.overlay_rows(composer_height);
        let transcript_height = self.height - composer_height - 1 - palette.len();
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
        for (index, line) in palette.iter().enumerate() {
            rows[transcript_height + index] = fit(line, self.width);
        }
        let composer_row_start = transcript_height + palette.len();
        for (index, line) in composer
            .iter()
            .skip(composer_start)
            .take(composer_height)
            .enumerate()
        {
            rows[composer_row_start + index] = fit(
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
                composer_row_start + cursor_row - composer_start,
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
    for grapheme in text.graphemes(true) {
        if grapheme == "\n" {
            rows.push(String::new());
            column = 0;
            continue;
        }
        let text = if grapheme == "\t" { "    " } else { grapheme };
        let cells = UnicodeWidthStr::width(text);
        if column + cells > width {
            rows.push(String::new());
            column = 0;
        }
        rows.last_mut().unwrap().push_str(text);
        column += cells;
    }
    if column == width {
        rows.push(String::new());
    }
    rows
}

fn fit(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut cells = 0;
    for grapheme in text.graphemes(true) {
        let size = UnicodeWidthStr::width(grapheme);
        if grapheme.chars().any(char::is_control) || cells + size > width {
            break;
        }
        result.push_str(grapheme);
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
    fn palette_ranks_aliases_and_previews_the_canonical_command() {
        let mut view = viewport();
        view.key(Key::Text("/eval".into()));
        let grid = view.render();
        let rendered = grid.rows.join("\n");
        assert!(rendered.contains("Command Palette · /eval → /evaluation"));
        assert!(rendered.contains("› /evaluation"));
        assert!(rendered.contains("Run or inspect skill evaluations."));
        assert!(rendered.contains("Arguments: OPERATION [ARGUMENTS]"));
        assert!(rendered.contains("Read-only"), "{rendered}");
        assert_eq!(
            rendered.matches("› /evaluation").count(),
            1,
            "the compatibility alias must not create a second row"
        );
        assert_grid(&grid, 60, 16);

        let mut view = viewport();
        view.key(Key::Text("/skill".into()));
        assert!(view.render().rows.join("\n").contains("Plan + approval"));
    }

    #[test]
    fn help_opens_the_unfiltered_palette_and_argument_commands_complete_in_place() {
        let mut view = viewport();
        view.key(Key::Text("/help".into()));
        assert_eq!(view.key(Key::Enter).as_deref(), Some("/help"));
        view.open_palette_browse();
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("Command Palette · Browse all commands"));
        assert!(rendered.contains("/cancel"));
        for _ in 0..14 {
            view.key(Key::Down);
        }
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("/command-skill"));

        let mut view = viewport();
        view.key(Key::Text("/eval".into()));
        assert_eq!(view.key(Key::Enter), None);
        let grid = view.render();
        assert!(grid
            .rows
            .iter()
            .any(|row| row.starts_with("› /evaluation ")));
        assert_eq!(grid.cursor, Some((14, 14)));
    }

    #[test]
    fn unmatched_slash_input_stays_editable_and_is_not_submitted() {
        let mut view = viewport();
        view.key(Key::Text("/no-such-command".into()));
        let before = view.render().rows.join("\n");
        assert!(before.contains("No matching Slash Commands"));
        assert_eq!(view.key(Key::Enter), None);
        assert!(view.render().rows.join("\n").contains("/no-such-command"));
    }

    #[test]
    fn incomplete_command_arguments_stay_editable_instead_of_running_another_command() {
        for input in ["/model profile", "/profile status extra"] {
            let mut view = viewport();
            view.key(Key::Text(input.into()));
            assert_eq!(view.key(Key::Enter), None);
            assert_eq!(view.composer, input);
        }
    }

    #[test]
    fn a_fully_typed_compatibility_alias_submits_its_canonical_command() {
        let mut view = viewport();
        view.key(Key::Text("/eval run skill:example".into()));
        assert_eq!(
            view.key(Key::Enter).as_deref(),
            Some("/evaluation run skill:example")
        );
    }

    #[test]
    fn active_operation_disables_unsafe_commands_but_keeps_safe_commands_selectable() {
        let mut view = viewport();
        view.key(Key::Text("/".into()));
        view.set_operation_active(true);
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("/status"));
        assert!(rendered.contains("/cancel"));
        assert!(rendered.contains("/help"));
        assert!(rendered.contains("/skill · Available when idle"));

        let mut view = viewport();
        view.set_operation_active(true);
        view.key(Key::Text("/st".into()));
        assert_eq!(view.key(Key::Enter).as_deref(), Some("/status"));

        let mut view = viewport();
        view.set_operation_active(true);
        view.key(Key::Text("/skill remove example".into()));
        assert_eq!(view.key(Key::Enter), None);
        assert!(view
            .render()
            .rows
            .join("\n")
            .contains("/skill remove example"));
    }

    #[test]
    fn escape_clears_the_filter_then_restores_exact_composer_and_cursor() {
        let mut view = viewport();
        view.key(Key::Text("    ".into()));
        view.key(Key::Left);
        view.key(Key::Left);
        view.key(Key::Text("/eval".into()));
        view.key(Key::Escape);
        assert_eq!(view.composer, "  /  ");
        assert_eq!(view.cursor, 3);
        view.key(Key::Escape);
        let grid = view.render();
        assert_eq!(view.composer, "    ");
        assert_eq!(view.cursor, 2);
        assert_eq!(grid.cursor, Some((4, 14)));
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
    fn pages_use_the_visible_transcript_height_and_resume_at_the_bottom() {
        let mut view = viewport();
        for index in 0..50 {
            view.content(&format!("row {index}"));
        }
        assert!(view.render().rows[13].starts_with("row 49"));
        view.key(Key::PageUp);
        let grid = view.render();
        assert!(grid.rows[0].starts_with("row 22"));
        assert!(grid.rows[15].starts_with("Paused ·"));
        view.content("row 50");
        assert!(view.render().rows[0].starts_with("row 22"));
        assert!(view.render().rows[15].starts_with("Paused · new output"));
        view.key(Key::PageDown);
        assert!(view.render().rows[15].starts_with("Paused · new output"));
        view.key(Key::PageDown);
        assert!(view.render().rows[15].starts_with("Idle ·"));
        view.content("row 51");
        assert!(view.render().rows[13].starts_with("row 51"));
    }

    #[test]
    fn position_indicator_only_appears_when_scrollable_and_tracks_position() {
        let mut view = viewport();
        for index in 0..14 {
            view.content(&format!("row {index}"));
        }
        assert!(view.render().rows[..14]
            .iter()
            .all(|row| row.ends_with(' ')));
        view.content("row 14");
        let grid = view.render();
        assert!(grid.rows[..13].iter().all(|row| row.ends_with('│')));
        assert!(grid.rows[13].ends_with('█'));
        view.key(Key::PageUp);
        assert!(view.render().rows[0].ends_with('█'));
        view.key(Key::End);
        assert!(view.render().rows[13].ends_with('█'));
        assert!(view.render().rows[15].starts_with("Idle ·"));
        assert_grid(&view.render(), 60, 16);
    }

    #[test]
    fn resizing_to_fit_the_transcript_resumes_live_output() {
        let mut view = viewport();
        for index in 0..20 {
            view.content(&format!("row {index}"));
        }
        view.key(Key::PageUp);
        view.content("new output");
        view.resize(60, 30);
        let grid = view.render();
        assert!(grid.rows[29].starts_with("Idle ·"));
        assert!(grid.rows.iter().all(|row| !row.ends_with('█')));
        view.resize(60, 16);
        view.content("live output");
        assert!(view.render().rows[13].starts_with("live output"));
        assert!(view.render().rows[15].starts_with("Idle ·"));
    }

    #[test]
    fn submitting_a_multiline_composer_resumes_when_the_transcript_fits() {
        let mut view = viewport();
        for index in 0..14 {
            view.content(&format!("row {index}"));
        }
        view.key(Key::Paste("one\ntwo\nthree\nfour\nfive".into()));
        view.key(Key::PageUp);
        assert!(view.render().rows[15].starts_with("Paused ·"));
        view.key(Key::Enter);
        assert!(view.render().rows[15].starts_with("Idle ·"));
        view.content("live output");
        assert!(view.render().rows[13].starts_with("live output"));
    }

    #[test]
    fn shrinking_working_output_to_fit_resumes_live_output() {
        let mut view = viewport();
        view.present(Event::Working, &"progress\n".repeat(20));
        view.key(Key::PageUp);
        view.present(Event::Done, "finished");
        assert!(view.render().rows[15].starts_with("Idle ·"));
        view.content("live output");
        assert!(view.render().rows[1].starts_with("live output"));
        assert!(view.render().rows[15].starts_with("Idle ·"));
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
    fn paused_page_stays_anchored_when_working_rows_reflow() {
        let mut view = viewport();
        for index in 0..30 {
            view.content(&format!("row {index}"));
        }
        view.present(Event::Working, "search");
        view.key(Key::PageUp);
        let first = view.render().rows[0].clone();
        view.present(Event::Working, &"long progress ".repeat(20));
        assert_eq!(view.render().rows[0], first);
        assert!(view.render().rows[15].contains("Paused · new output"));
        view.present(Event::Done, "finished");
        assert_eq!(view.render().rows[0], first);

        let mut view = viewport();
        view.present(Event::Working, "search");
        for index in 0..40 {
            view.content(&format!("row {index}"));
        }
        view.key(Key::PageUp);
        let first = view.render().rows[0].clone();
        view.present(Event::Done, &"long outcome ".repeat(20));
        assert_eq!(view.render().rows[0], first);
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

    #[test]
    fn composed_unicode_keeps_cell_width_and_editing_boundaries() {
        for grapheme in ["👩‍💻", "🇳🇱", "e\u{301}", "界"] {
            let mut view = viewport();
            view.footer.profile = grapheme.repeat(4);
            view.footer.model = grapheme.repeat(4);
            view.present(Event::Tasc, &grapheme.repeat(60));
            view.key(Key::Paste(grapheme.repeat(60)));
            assert_grid(&view.render(), 60, 16);
            let (column, row) = view.render().cursor.unwrap();
            assert!(column < 60 && row < 15);
            view.key(Key::Enter);
            view.key(Key::Text(grapheme.into()));
            view.key(Key::Backspace);
            assert_eq!(view.key(Key::Enter).unwrap(), "");
        }
    }

    fn picker_view(models: &[&str]) -> Viewport {
        let mut view = viewport();
        view.key(Key::Text("draft".into()));
        view.key(Key::Left);
        view.open_model_picker();
        let models = models.iter().map(|m| m.to_string()).collect::<Vec<_>>();
        view.set_model_picker_models(&models, "m2", Some("m3"));
        view
    }

    #[test]
    fn model_picker_marks_session_and_profile_independently_and_is_bounded() {
        let models = (1..=20).map(|n| format!("m{n}")).collect::<Vec<_>>();
        let refs = models.iter().map(String::as_str).collect::<Vec<_>>();
        let mut view = picker_view(&refs);
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("Model Picker"));
        assert!(rendered.contains("auto"));
        assert!(rendered.contains("m2 · active session"), "{rendered}");
        assert!(rendered.contains("m3 · profile default"), "{rendered}");
        assert!(rendered.matches("· active session").count() == 1);
        let choices = rendered
            .lines()
            .skip_while(|line| !line.contains("Model Picker"))
            .skip(1)
            .take_while(|line| !line.contains("Selection"))
            .count();
        assert!((1..=PICKER_MAX_ROWS).contains(&choices), "{rendered}");
        for _ in 0..12 {
            view.key(Key::Down);
        }
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("› m12"), "{rendered}");
        assert!(rendered.contains("Selection 13/21"));
        view.key(Key::PageUp);
        assert!(view.render().rows.join("\n").contains("Selection 5/21"));
        assert_grid(&view.render(), 60, 16);
    }

    fn inventory_entries(count: usize) -> Vec<(String, String)> {
        (1..=count)
            .map(|n| (format!("owner/skill-{n}"), format!("Skill {n}")))
            .collect()
    }

    fn inventory_view(count: usize, multi: bool) -> Viewport {
        let mut view = viewport();
        view.open_inventory_picker("skill remove", multi, &inventory_entries(count));
        view
    }

    #[test]
    fn inventory_picker_renders_only_visible_window_with_range_and_total() {
        let mut view = inventory_view(500, true);
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("Showing 1-8 of 500"));
        assert!(rendered.contains("0 selected"));
        assert!(rendered.contains("owner/skill-8"));
        assert!(!rendered.contains("owner/skill-9 "));
        for _ in 0..10 {
            view.key(Key::Down);
        }
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("Showing 4-11 of 500"));
    }

    #[test]
    fn inventory_selection_survives_filtering_and_scrolling_by_stable_id() {
        let mut view = inventory_view(30, true);
        view.key(Key::Text(" ".into()));
        for _ in 0..19 {
            view.key(Key::Down);
        }
        view.key(Key::Text(" ".into()));
        view.key(Key::Text("skill-2".into()));
        view.key(Key::Text(" ".into()));
        view.key(Key::Escape);
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("3 selected"), "{rendered}");
        let command = view.key(Key::Enter).unwrap();
        assert_eq!(
            command,
            "/skill remove owner/skill-1 owner/skill-2 owner/skill-20"
        );
        assert!(!view.picker_open());
    }

    #[test]
    fn inventory_clear_and_empty_confirm_do_not_submit() {
        let mut view = inventory_view(3, true);
        view.key(Key::Text(" ".into()));
        view.key(Key::Delete);
        assert!(view.render().rows.join("\n").contains("0 selected"));
        assert!(view.key(Key::Enter).is_none());
        assert!(view.picker_open());
        assert!(view
            .render()
            .rows
            .join("\n")
            .contains("Select at least one"));
    }

    #[test]
    fn inventory_single_select_returns_command_line_only() {
        let mut view = inventory_view(3, false);
        view.key(Key::Down);
        assert_eq!(view.key(Key::Enter).unwrap(), "/skill remove owner/skill-2");
        assert!(!view.picker_open());
    }

    #[test]
    fn picker_filters_and_commits_through_scope_picker() {
        let mut view = picker_view(&["alpha", "beta"]);
        view.key(Key::Text("BET".into()));
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("filter: BET") && rendered.contains("› beta"));
        assert!(!rendered.contains("alpha"));
        assert_eq!(view.key(Key::Enter), None);
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("Scope Picker · beta"));
        assert!(rendered.contains("This Agent Session") && rendered.contains("Profile default"));
        view.key(Key::Down);
        assert_eq!(view.key(Key::Enter).as_deref(), Some("/model profile beta"));
        assert!(!view.picker_open());

        let mut view = picker_view(&["alpha"]);
        view.key(Key::Down);
        view.key(Key::Enter);
        assert_eq!(view.key(Key::Enter).as_deref(), Some("/model alpha"));
    }

    #[test]
    fn picker_escape_clears_filter_then_unwinds_restoring_composer() {
        let mut view = picker_view(&["alpha", "beta"]);
        view.key(Key::Text("zzz".into()));
        assert!(view.render().rows.join("\n").contains("No matches"));
        view.key(Key::Escape);
        assert!(view.picker_open());
        assert!(view.render().rows.join("\n").contains("alpha"));
        view.key(Key::Enter);
        assert!(view.render().rows.join("\n").contains("Scope Picker"));
        view.key(Key::Escape);
        assert!(view.render().rows.join("\n").contains("Model Picker"));
        view.key(Key::Escape);
        assert!(!view.picker_open());
        let grid = view.render();
        assert!(grid.rows.join("\n").contains("› draft"));
        assert_eq!(grid.cursor.map(|(column, _)| column), Some(2 + 4));
        view.key(Key::Text("!".into()));
        assert!(view.render().rows.join("\n").contains("› draf!t"));
    }

    #[test]
    fn picker_failure_renders_in_place_without_choices() {
        let mut view = viewport();
        view.open_model_picker();
        assert!(view.render().rows.join("\n").contains("Loading models…"));
        view.fail_model_picker("offline");
        let rendered = view.render().rows.join("\n");
        assert!(rendered.contains("Models unavailable: offline"));
        assert_eq!(view.key(Key::Enter), None);
        assert!(view.picker_open());
        view.key(Key::Escape);
        assert!(!view.picker_open());
    }
}
