//! In-TUI configuration editor.
//!
//! The editor is a small form rendered as a modal over the main screen. It owns
//! a copy of the configuration, so cancelling never touches the file on disk.

use crate::config::{normalise_binding, Config, KeyField};
use crate::headset::SIDETONE_MAX;
use crossterm::event::{KeyCode, KeyEvent};

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
        let message = config.keys.conflict().map(|conflict| conflict.to_string());

        Self {
            config,
            original,
            selected: 0,
            editing_sidetone: false,
            sidetone_buffer: String::new(),
            sidetone_fresh: false,
            message,
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
            KeyCode::F(2) => self.save(),
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
            Row::Save => self.save(),
            Row::Cancel => EditorOutcome::Cancel,
        }
    }

    fn save(&mut self) -> EditorOutcome {
        match self.config.keys.conflict() {
            Some(conflict) => {
                self.message = Some(conflict.to_string());
                EditorOutcome::Continue
            }
            None => EditorOutcome::Save(self.config.clone()),
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
                let key = normalise_binding(character);
                match self.config.keys.binding_available(field, key) {
                    Ok(()) => {
                        field.set(&mut self.config.keys, key);
                        // A different field may still carry a conflict loaded
                        // from an older config, so re-check the whole set.
                        self.message = self
                            .config
                            .keys
                            .conflict()
                            .map(|conflict| conflict.to_string());
                    }
                    Err(conflict) => self.message = Some(conflict.to_string()),
                }
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
                    self.save()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeyConfig;
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

        editor.handle_key(character('z'));

        assert_eq!(editor.config.keys.dec1, 'z');
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

        editor.handle_key(character('z'));

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
    fn duplicate_bindings_are_rejected() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Quit));

        editor.handle_key(character('a'));

        assert_eq!(editor.config.keys.quit, 'q');
        assert_eq!(editor.config.keys.dec1, 'a');
        assert_eq!(
            editor.message.as_deref(),
            Some("quit: 'a' is already bound to dec1")
        );
        assert!(!editor.is_dirty());
    }

    #[test]
    fn reserved_shortcut_keys_are_rejected() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Quit));

        editor.handle_key(character('j'));

        assert_eq!(editor.config.keys.quit, 'q');
        assert_eq!(
            editor.message.as_deref(),
            Some("quit: 'j' is reserved for sidetone control")
        );
        assert!(!editor.is_dirty());
    }

    #[test]
    fn a_valid_rebind_clears_a_conflict_message() {
        let mut editor = editor();
        select(&mut editor, Row::Key(KeyField::Quit));

        editor.handle_key(character('a'));
        assert!(editor.message.is_some());

        editor.handle_key(character('z'));

        assert_eq!(editor.config.keys.quit, 'z');
        assert_eq!(editor.message, None);
    }

    #[test]
    fn an_invalid_config_cannot_be_saved() {
        let mut config = Config::default();
        config.keys.dec10 = 'a';
        let mut editor = ConfigEditor::new(config);

        assert_eq!(
            editor.message.as_deref(),
            Some("dec10: 'a' is already bound to dec1")
        );
        assert_eq!(
            editor.handle_key(press(KeyCode::F(2))),
            EditorOutcome::Continue
        );
        assert!(editor.message.is_some());
    }

    #[test]
    fn fixing_one_conflict_keeps_other_conflicts_visible() {
        let config = Config {
            keys: KeyConfig {
                dec1: 'h',
                ..KeyConfig::default()
            },
            ..Config::default()
        };
        let mut editor = ConfigEditor::new(config);

        select(&mut editor, Row::Key(KeyField::Dec10));
        editor.handle_key(character('z'));
        assert_eq!(
            editor.message.as_deref(),
            Some("dec1: 'h' is reserved for sidetone control")
        );

        select(&mut editor, Row::Key(KeyField::Dec1));
        editor.handle_key(character('b'));
        assert_eq!(editor.message, None);
    }

    #[test]
    fn fixing_a_loaded_conflict_allows_saving() {
        let mut config = Config::default();
        config.keys.dec10 = 'a';
        let mut editor = ConfigEditor::new(config);

        select(&mut editor, Row::Key(KeyField::Dec10));
        editor.handle_key(character('z'));

        assert_eq!(
            editor.handle_key(press(KeyCode::F(2))),
            EditorOutcome::Save(editor.config.clone())
        );
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
        editor.handle_key(character('z'));
        select(&mut editor, Row::Save);

        let outcome = editor.handle_key(press(KeyCode::Enter));

        match outcome {
            EditorOutcome::Save(config) => assert_eq!(config.keys.dec1, 'z'),
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
        editor.handle_key(character('z'));

        match editor.handle_key(press(KeyCode::F(2))) {
            EditorOutcome::Save(config) => assert_eq!(config.keys.dec1, 'z'),
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

        editor.handle_key(character('z'));
        assert!(editor.is_dirty());

        editor.handle_key(character('a'));
        assert!(!editor.is_dirty());
    }
}
