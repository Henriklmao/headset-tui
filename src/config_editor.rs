//! In-TUI configuration editor.
//!
//! The editor is a small form rendered as a modal over the main screen. It owns
//! a copy of the configuration, so cancelling never touches the file on disk.

use crate::config::{Config, KeyConfig};
use crate::headset::SIDETONE_MAX;
use crossterm::event::{KeyCode, KeyEvent};

/// One configurable keybinding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyField {
    Dec1,
    Dec10,
    Inc1,
    Inc10,
    Full,
    None,
    Config,
    Verbose,
    Quit,
}

impl KeyField {
    /// Every keybinding, in the order the editor lists them.
    #[cfg(test)]
    pub const ALL: [KeyField; 9] = [
        KeyField::Dec1,
        KeyField::Dec10,
        KeyField::Inc1,
        KeyField::Inc10,
        KeyField::Full,
        KeyField::None,
        KeyField::Config,
        KeyField::Verbose,
        KeyField::Quit,
    ];

    /// Human readable description, matching the `--config` dialog.
    pub fn label(self) -> &'static str {
        match self {
            KeyField::Dec1 => "Decrease sidetone by 1",
            KeyField::Dec10 => "Decrease sidetone by 10",
            KeyField::Inc1 => "Increase sidetone by 1",
            KeyField::Inc10 => "Increase sidetone by 10",
            KeyField::Full => "Set sidetone to full (128)",
            KeyField::None => "Set sidetone to none (0)",
            KeyField::Config => "Open configuration menu",
            KeyField::Verbose => "Toggle verbose mode",
            KeyField::Quit => "Quit the application",
        }
    }

    /// The character currently bound to this field.
    pub fn get(self, keys: &KeyConfig) -> char {
        match self {
            KeyField::Dec1 => keys.dec1,
            KeyField::Dec10 => keys.dec10,
            KeyField::Inc1 => keys.inc1,
            KeyField::Inc10 => keys.inc10,
            KeyField::Full => keys.full,
            KeyField::None => keys.none,
            KeyField::Config => keys.config,
            KeyField::Verbose => keys.verbose,
            KeyField::Quit => keys.quit,
        }
    }

    /// Bind `value` to this field.
    pub fn set(self, keys: &mut KeyConfig, value: char) {
        match self {
            KeyField::Dec1 => keys.dec1 = value,
            KeyField::Dec10 => keys.dec10 = value,
            KeyField::Inc1 => keys.inc1 = value,
            KeyField::Inc10 => keys.inc10 = value,
            KeyField::Full => keys.full = value,
            KeyField::None => keys.none = value,
            KeyField::Config => keys.config = value,
            KeyField::Verbose => keys.verbose = value,
            KeyField::Quit => keys.quit = value,
        }
    }
}

/// One selectable row in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    DefaultSidetone,
    Key(KeyField),
    Save,
    Cancel,
}

/// What the event loop should do after the editor handled a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorOutcome {
    Continue,
    Save(Config),
    Cancel,
}

/// State of the modal configuration form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEditor {
    pub config: Config,
    /// Configuration loaded when the editor opened, used for dirty tracking.
    pub original: Config,
    selected: usize,
    editing_sidetone: bool,
    sidetone_buffer: String,
    /// True when the buffer was prefilled with Enter and the next digit should
    /// replace it instead of appending.
    sidetone_fresh: bool,
    pub message: Option<String>,
}

impl ConfigEditor {
    /// Rows in display order.
    pub const ROWS: [Row; 12] = [
        Row::DefaultSidetone,
        Row::Key(KeyField::Dec1),
        Row::Key(KeyField::Dec10),
        Row::Key(KeyField::Inc1),
        Row::Key(KeyField::Inc10),
        Row::Key(KeyField::Full),
        Row::Key(KeyField::None),
        Row::Key(KeyField::Config),
        Row::Key(KeyField::Verbose),
        Row::Key(KeyField::Quit),
        Row::Save,
        Row::Cancel,
    ];

    pub fn new(config: Config) -> Self {
        let original = config.clone();

        Self {
            config,
            original,
            selected: 0,
            editing_sidetone: false,
            sidetone_buffer: String::new(),
            sidetone_fresh: false,
            message: None,
        }
    }

    /// Index of the selected row.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The currently selected row.
    pub fn selected_row(&self) -> Row {
        Self::ROWS[self.selected]
    }

    /// Whether the sidetone value is currently being typed.
    pub fn is_editing_sidetone(&self) -> bool {
        self.editing_sidetone
    }

    /// Current sidetone edit buffer.
    pub fn sidetone_buffer(&self) -> &str {
        &self.sidetone_buffer
    }

    /// Whether anything was changed since the editor opened.
    pub fn is_dirty(&self) -> bool {
        self.config != self.original
    }

    fn next(&mut self) {
        self.selected = (self.selected + 1) % Self::ROWS.len();
    }

    fn previous(&mut self) {
        self.selected = (self.selected + Self::ROWS.len() - 1) % Self::ROWS.len();
    }

    /// Apply one key press to the form.
    pub fn handle_key(&mut self, key: KeyEvent) -> EditorOutcome {
        if self.editing_sidetone {
            return self.handle_sidetone_edit(key);
        }

        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.previous();
                EditorOutcome::Continue
            }
            KeyCode::Down | KeyCode::Tab => {
                self.next();
                EditorOutcome::Continue
            }
            KeyCode::Home => {
                self.selected = 0;
                EditorOutcome::Continue
            }
            KeyCode::End => {
                self.selected = Self::ROWS.len() - 1;
                EditorOutcome::Continue
            }
            KeyCode::Enter => self.activate(),
            KeyCode::Esc => EditorOutcome::Cancel,
            KeyCode::F(2) => EditorOutcome::Save(self.config.clone()),
            KeyCode::Char(character) if !character.is_control() => {
                if character == ' ' && matches!(self.selected_row(), Row::Save | Row::Cancel) {
                    self.activate()
                } else {
                    self.type_character(character);
                    EditorOutcome::Continue
                }
            }
            _ => EditorOutcome::Continue,
        }
    }

    fn activate(&mut self) -> EditorOutcome {
        match self.selected_row() {
            Row::DefaultSidetone => {
                self.begin_sidetone_edit();
                EditorOutcome::Continue
            }
            Row::Key(_) => {
                self.next();
                EditorOutcome::Continue
            }
            Row::Save => EditorOutcome::Save(self.config.clone()),
            Row::Cancel => EditorOutcome::Cancel,
        }
    }

    fn type_character(&mut self, character: char) {
        match self.selected_row() {
            Row::DefaultSidetone => {
                if character.is_ascii_digit() {
                    self.editing_sidetone = true;
                    self.sidetone_buffer = character.to_string();
                    self.sidetone_fresh = false;
                    self.message = None;
                }
            }
            Row::Key(field) => {
                field.set(&mut self.config.keys, normalise(character));
                self.message = None;
            }
            // Save and Cancel are activated by Enter or Space in `handle_key`;
            // other characters are ignored here.
            Row::Save | Row::Cancel => {}
        }
    }

    fn begin_sidetone_edit(&mut self) {
        self.editing_sidetone = true;
        self.sidetone_buffer = self.config.default_sidetone.to_string();
        // Let the first typed digit replace the prefilled value.
        self.sidetone_fresh = true;
        self.message = None;
    }

    fn handle_sidetone_edit(&mut self, key: KeyEvent) -> EditorOutcome {
        match key.code {
            KeyCode::Char(character) if character.is_ascii_digit() => {
                let buffer = if self.sidetone_fresh {
                    self.sidetone_fresh = false;
                    String::new()
                } else {
                    self.sidetone_buffer.clone()
                };

                let candidate = format!("{buffer}{character}");
                match candidate.parse::<u16>() {
                    Ok(value) if value <= u16::from(SIDETONE_MAX) => {
                        self.sidetone_buffer = candidate;
                        self.message = None;
                    }
                    _ => {
                        self.message =
                            Some(format!("Sidetone must be between 0 and {SIDETONE_MAX}"));
                    }
                }
                EditorOutcome::Continue
            }
            KeyCode::Backspace => {
                self.sidetone_buffer.pop();
                self.sidetone_fresh = false;
                self.message = None;
                EditorOutcome::Continue
            }
            KeyCode::Enter => {
                if self.commit_sidetone() {
                    self.next();
                }
                EditorOutcome::Continue
            }
            KeyCode::Esc => {
                self.editing_sidetone = false;
                self.sidetone_fresh = false;
                self.sidetone_buffer.clear();
                self.message = None;
                EditorOutcome::Continue
            }
            KeyCode::F(2) => {
                if self.commit_sidetone() {
                    EditorOutcome::Save(self.config.clone())
                } else {
                    EditorOutcome::Continue
                }
            }
            _ => EditorOutcome::Continue,
        }
    }

    /// Validate and store the sidetone edit buffer. Returns whether it was
    /// committed.
    fn commit_sidetone(&mut self) -> bool {
        match self.sidetone_buffer.parse::<u16>() {
            Ok(value) if value <= u16::from(SIDETONE_MAX) => {
                self.config.default_sidetone = value as u8;
                self.editing_sidetone = false;
                self.sidetone_fresh = false;
                self.message = None;
                true
            }
            _ => {
                self.message = Some(format!("Sidetone must be between 0 and {SIDETONE_MAX}"));
                false
            }
        }
    }
}

/// Lowercase a binding so uppercase and lowercase input behave the same way.
fn normalise(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn character(value: char) -> KeyEvent {
        press(KeyCode::Char(value))
    }

    fn editor() -> ConfigEditor {
        ConfigEditor::new(Config::default())
    }

    fn select(editor: &mut ConfigEditor, row: Row) {
        editor.selected = ConfigEditor::ROWS
            .iter()
            .position(|candidate| *candidate == row)
            .expect("row exists");
    }

    // -- KeyField ----------------------------------------------------------

    #[test]
    fn key_fields_cover_every_binding() {
        let mut keys = KeyConfig::default();

        for (index, field) in KeyField::ALL.iter().enumerate() {
            let value = char::from(b'a' + index as u8);
            field.set(&mut keys, value);
            assert_eq!(field.get(&keys), value, "{field:?}");
        }
    }

    #[test]
    fn key_field_labels_match_the_config_dialog() {
        assert_eq!(KeyField::Dec1.label(), "Decrease sidetone by 1");
        assert_eq!(KeyField::Dec10.label(), "Decrease sidetone by 10");
        assert_eq!(KeyField::Inc1.label(), "Increase sidetone by 1");
        assert_eq!(KeyField::Inc10.label(), "Increase sidetone by 10");
        assert_eq!(KeyField::Full.label(), "Set sidetone to full (128)");
        assert_eq!(KeyField::None.label(), "Set sidetone to none (0)");
        assert_eq!(KeyField::Config.label(), "Open configuration menu");
        assert_eq!(KeyField::Verbose.label(), "Toggle verbose mode");
        assert_eq!(KeyField::Quit.label(), "Quit the application");
    }

    // -- construction and navigation --------------------------------------

    #[test]
    fn new_editor_starts_on_the_default_sidetone_row() {
        let editor = editor();

        assert_eq!(editor.selected_row(), Row::DefaultSidetone);
        assert!(!editor.is_dirty());
        assert!(!editor.is_editing_sidetone());
        assert_eq!(editor.message, None);
    }

    #[test]
    fn rows_are_in_the_expected_order() {
        assert_eq!(ConfigEditor::ROWS[0], Row::DefaultSidetone);
        assert_eq!(ConfigEditor::ROWS[1], Row::Key(KeyField::Dec1));
        assert_eq!(ConfigEditor::ROWS[10], Row::Save);
        assert_eq!(ConfigEditor::ROWS[11], Row::Cancel);
    }

    #[test]
    fn down_moves_to_the_next_row() {
        let mut editor = editor();

        editor.handle_key(press(KeyCode::Down));

        assert_eq!(editor.selected_row(), Row::Key(KeyField::Dec1));
    }

    #[test]
    fn up_wraps_to_the_last_row() {
        let mut editor = editor();

        editor.handle_key(press(KeyCode::Up));

        assert_eq!(editor.selected_row(), Row::Cancel);
    }

    #[test]
    fn down_wraps_to_the_first_row() {
        let mut editor = editor();
        select(&mut editor, Row::Cancel);

        editor.handle_key(press(KeyCode::Down));

        assert_eq!(editor.selected_row(), Row::DefaultSidetone);
    }

    #[test]
    fn tab_and_backtab_navigate() {
        let mut editor = editor();

        editor.handle_key(press(KeyCode::Tab));
        assert_eq!(editor.selected_row(), Row::Key(KeyField::Dec1));

        editor.handle_key(press(KeyCode::BackTab));
        assert_eq!(editor.selected_row(), Row::DefaultSidetone);

        editor.handle_key(press(KeyCode::BackTab));
        assert_eq!(editor.selected_row(), Row::Cancel);
    }

    #[test]
    fn home_and_end_jump_to_the_edges() {
        let mut editor = editor();

        editor.handle_key(press(KeyCode::End));
        assert_eq!(editor.selected_row(), Row::Cancel);

        editor.handle_key(press(KeyCode::Home));
        assert_eq!(editor.selected_row(), Row::DefaultSidetone);
    }

    // -- keybindings -------------------------------------------------------

    #[test]
    fn typing_a_character_rebinds_the_selected_key() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Dec1));

        editor.handle_key(character('j'));

        assert_eq!(editor.config.keys.dec1, 'j');
        assert!(editor.is_dirty());
    }

    #[test]
    fn uppercase_input_is_lowercased() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Inc1));

        editor.handle_key(character('X'));

        assert_eq!(editor.config.keys.inc1, 'x');
    }

    #[test]
    fn space_can_be_bound_to_a_key() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Full));

        editor.handle_key(character(' '));

        assert_eq!(editor.config.keys.full, ' ');
    }

    #[test]
    fn control_characters_are_ignored() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Dec1));

        editor.handle_key(character('\u{1}'));

        assert_eq!(editor.config.keys.dec1, 'a');
        assert!(!editor.is_dirty());
    }

    #[test]
    fn other_bindings_are_untouched_by_a_rebind() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Dec1));

        editor.handle_key(character('j'));

        assert_eq!(editor.config.keys.dec10, 's');
        assert_eq!(editor.config.keys.inc1, 'd');
    }

    #[test]
    fn enter_on_a_key_row_advances() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Dec1));

        editor.handle_key(press(KeyCode::Enter));

        assert_eq!(editor.selected_row(), Row::Key(KeyField::Dec10));
    }

    #[test]
    fn duplicate_bindings_are_allowed() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Quit));

        editor.handle_key(character('a'));

        assert_eq!(editor.config.keys.quit, 'a');
        assert_eq!(editor.config.keys.dec1, 'a');
    }

    // -- default sidetone --------------------------------------------------

    #[test]
    fn typing_a_digit_starts_editing_the_sidetone() {
        let mut editor = editor();

        editor.handle_key(character('6'));

        assert!(editor.is_editing_sidetone());
        assert_eq!(editor.sidetone_buffer(), "6");
    }

    #[test]
    fn digits_build_a_multi_digit_sidetone() {
        let mut editor = editor();

        for value in ['1', '2', '8'] {
            editor.handle_key(character(value));
        }

        assert_eq!(editor.sidetone_buffer(), "128");
    }

    #[test]
    fn out_of_range_digits_are_rejected() {
        let mut editor = editor();

        for value in ['1', '2', '9'] {
            editor.handle_key(character(value));
        }

        assert_eq!(editor.sidetone_buffer(), "12");
        assert_eq!(
            editor.message.as_deref(),
            Some("Sidetone must be between 0 and 128")
        );
    }

    #[test]
    fn backspace_removes_the_last_digit() {
        let mut editor = editor();

        for value in ['1', '2', '8'] {
            editor.handle_key(character(value));
        }
        editor.handle_key(press(KeyCode::Backspace));

        assert_eq!(editor.sidetone_buffer(), "12");
    }

    #[test]
    fn enter_commits_the_sidetone_and_advances() {
        let mut editor = editor();

        for value in ['6', '4'] {
            editor.handle_key(character(value));
        }
        editor.handle_key(press(KeyCode::Enter));

        assert_eq!(editor.config.default_sidetone, 64);
        assert!(!editor.is_editing_sidetone());
        assert_eq!(editor.selected_row(), Row::Key(KeyField::Dec1));
        assert!(editor.is_dirty());
    }

    #[test]
    fn enter_with_an_empty_buffer_is_rejected() {
        let mut editor = editor();
        select(&mut editor, Row::DefaultSidetone);
        editor.handle_key(press(KeyCode::Enter));
        editor.handle_key(press(KeyCode::Backspace));
        editor.handle_key(press(KeyCode::Enter));

        assert_eq!(
            editor.message.as_deref(),
            Some("Sidetone must be between 0 and 128")
        );
        assert!(editor.is_editing_sidetone());
    }

    #[test]
    fn escape_cancels_an_edit_without_changing_the_value() {
        let mut editor = editor();

        editor.handle_key(character('9'));
        editor.handle_key(press(KeyCode::Esc));

        assert_eq!(editor.config.default_sidetone, 0);
        assert!(!editor.is_editing_sidetone());
        assert!(!editor.is_dirty());
    }

    #[test]
    fn typing_after_enter_replaces_the_prefilled_value() {
        let mut editor = editor();
        editor.config.default_sidetone = 64;
        select(&mut editor, Row::DefaultSidetone);

        editor.handle_key(press(KeyCode::Enter));
        assert_eq!(editor.sidetone_buffer(), "64");

        editor.handle_key(character('2'));
        assert_eq!(editor.sidetone_buffer(), "2");
    }

    #[test]
    fn zero_is_a_valid_sidetone() {
        let mut editor = editor();
        editor.config.default_sidetone = 64;

        editor.handle_key(character('0'));
        editor.handle_key(press(KeyCode::Enter));

        assert_eq!(editor.config.default_sidetone, 0);
    }

    // -- save and cancel ---------------------------------------------------

    #[test]
    fn save_row_returns_the_edited_config() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Dec1));
        editor.handle_key(character('j'));
        select(&mut editor, Row::Save);

        let outcome = editor.handle_key(press(KeyCode::Enter));

        match outcome {
            EditorOutcome::Save(config) => assert_eq!(config.keys.dec1, 'j'),
            other => panic!("expected Save, got {other:?}"),
        }
    }

    #[test]
    fn cancel_row_returns_cancel() {
        let mut editor = editor();
        select(&mut editor, Row::Cancel);

        assert_eq!(
            editor.handle_key(press(KeyCode::Enter)),
            EditorOutcome::Cancel
        );
    }

    #[test]
    fn escape_returns_cancel_from_the_form() {
        assert_eq!(
            editor().handle_key(press(KeyCode::Esc)),
            EditorOutcome::Cancel
        );
    }

    #[test]
    fn f2_saves_from_the_form() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Dec1));
        editor.handle_key(character('j'));

        match editor.handle_key(press(KeyCode::F(2))) {
            EditorOutcome::Save(config) => assert_eq!(config.keys.dec1, 'j'),
            other => panic!("expected Save, got {other:?}"),
        }
    }

    #[test]
    fn f2_commits_an_in_progress_sidetone_edit() {
        let mut editor = editor();

        for value in ['6', '4'] {
            editor.handle_key(character(value));
        }

        match editor.handle_key(press(KeyCode::F(2))) {
            EditorOutcome::Save(config) => assert_eq!(config.default_sidetone, 64),
            other => panic!("expected Save, got {other:?}"),
        }
    }

    #[test]
    fn f2_does_not_save_an_invalid_sidetone_edit() {
        let mut editor = editor();
        // Enter prefills the current value; clear it so the buffer is invalid.
        editor.handle_key(press(KeyCode::Enter));
        editor.handle_key(press(KeyCode::Backspace));

        assert_eq!(
            editor.handle_key(press(KeyCode::F(2))),
            EditorOutcome::Continue
        );
        assert!(editor.is_editing_sidetone());
        assert!(editor.message.is_some());
    }

    #[test]
    fn space_activates_save_and_cancel_rows() {
        let mut editor = editor();
        select(&mut editor, Row::Save);
        assert!(matches!(
            editor.handle_key(character(' ')),
            EditorOutcome::Save(_)
        ));

        select(&mut editor, Row::Cancel);
        assert_eq!(editor.handle_key(character(' ')), EditorOutcome::Cancel);
    }

    #[test]
    fn dirty_tracking_compares_against_the_original() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Dec1));

        editor.handle_key(character('j'));
        assert!(editor.is_dirty());

        editor.handle_key(character('a'));
        assert!(!editor.is_dirty());
    }
}
