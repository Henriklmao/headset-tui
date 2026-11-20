//! Configuration and file locations.

use crate::cli::Mode;
use crate::headset::SIDETONE_MAX;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// Keybindings.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct KeyConfig {
    pub dec1: char,
    pub dec10: char,
    pub inc1: char,
    pub inc10: char,
    pub full: char,
    pub none: char,
    pub quit: char,
    pub config: char,
    pub verbose: char,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            dec1: 'a',
            dec10: 's',
            inc1: 'd',
            inc10: 'w',
            full: 'f',
            none: 'e',
            quit: 'q',
            config: 'c',
            verbose: 'v',
        }
    }
}

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
    /// Every keybinding, in the order the dialogs list them.
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

    /// Human readable description, matching the key binding dialogs.
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

    /// Name used in the TOML file and in conflict messages.
    pub fn name(self) -> &'static str {
        match self {
            KeyField::Dec1 => "dec1",
            KeyField::Dec10 => "dec10",
            KeyField::Inc1 => "inc1",
            KeyField::Inc10 => "inc10",
            KeyField::Full => "full",
            KeyField::None => "none",
            KeyField::Config => "config",
            KeyField::Verbose => "verbose",
            KeyField::Quit => "quit",
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

/// Characters reserved for the built-in sidetone shortcuts.
pub const RESERVED_BINDINGS: [char; 4] = ['h', 'j', 'k', 'l'];

/// Normalize a binding so uppercase and lowercase input compare equal.
pub fn normalise_binding(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

/// Why a key cannot be assigned to a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyConflict {
    /// The key is one of the fixed sidetone shortcuts.
    Reserved { field: KeyField, key: char },
    /// Another action already uses the key.
    Duplicate {
        field: KeyField,
        key: char,
        other: KeyField,
    },
}

impl fmt::Display for KeyConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyConflict::Reserved { field, key } => {
                write!(
                    f,
                    "{}: '{key}' is reserved for sidetone control",
                    field.name()
                )
            }
            KeyConflict::Duplicate { field, key, other } => {
                write!(
                    f,
                    "{}: '{key}' is already bound to {}",
                    field.name(),
                    other.name()
                )
            }
        }
    }
}

impl std::error::Error for KeyConflict {}

impl KeyConfig {
    /// Check whether `key` can be assigned to `field`.
    pub fn binding_available(&self, field: KeyField, key: char) -> Result<(), KeyConflict> {
        let key = normalise_binding(key);

        if RESERVED_BINDINGS.contains(&key) {
            return Err(KeyConflict::Reserved { field, key });
        }

        for other in KeyField::ALL {
            if other != field && normalise_binding(other.get(self)) == key {
                return Err(KeyConflict::Duplicate { field, key, other });
            }
        }

        Ok(())
    }

    /// The first conflicting binding in the configuration, if any.
    pub fn conflict(&self) -> Option<KeyConflict> {
        let mut seen: Vec<(char, KeyField)> = Vec::new();

        for field in KeyField::ALL {
            let key = normalise_binding(field.get(self));

            if RESERVED_BINDINGS.contains(&key) {
                return Some(KeyConflict::Reserved { field, key });
            }

            if let Some((_, other)) = seen.iter().find(|(seen_key, _)| *seen_key == key) {
                return Some(KeyConflict::Duplicate {
                    field,
                    key,
                    other: *other,
                });
            }

            seen.push((key, field));
        }

        None
    }
}

/// Persisted configuration.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub keys: KeyConfig,
    #[serde(default)]
    pub default_sidetone: u8,
}

/// `~/.config/headset-tui`.
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/headset-tui")
}

/// `~/.cache`.
pub fn cache_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".cache")
}

/// Path builder for a config file inside `dir`.
pub fn config_path_in(dir: &Path, mode: Mode) -> PathBuf {
    dir.join(format!("config{}.toml", mode.suffix()))
}

/// Path builder for a sidetone state file inside `dir`.
pub fn state_path_in(dir: &Path, mode: Mode) -> PathBuf {
    dir.join(format!("headset-tui-sidetone-state{}", mode.suffix()))
}

/// Path builder for the last-set sidetone level inside `dir`.
pub fn level_path_in(dir: &Path, mode: Mode) -> PathBuf {
    dir.join(format!("headset-tui-sidetone-level{}", mode.suffix()))
}

/// Config file used at runtime.
///
/// Test mode uses `config.test.toml`, so `--config --test` never touches the
/// real configuration.
pub fn config_path(mode: Mode) -> PathBuf {
    config_path_in(&config_dir(), mode)
}

/// Sidetone toggle state file used at runtime.
pub fn state_path(mode: Mode) -> PathBuf {
    state_path_in(&cache_dir(), mode)
}

/// File that remembers the sidetone level this app set last.
pub fn level_path(mode: Mode) -> PathBuf {
    level_path_in(&cache_dir(), mode)
}

/// Load the configuration, creating it with defaults when it does not exist.
pub fn load(mode: Mode) -> Config {
    load_or_create(&config_path(mode))
}

/// Load a config file, creating it with defaults when it does not exist yet.
///
/// An existing but unreadable config is left untouched and defaults are used
/// for this run, so a typo never destroys the user's bindings.
pub fn load_or_create(path: &Path) -> Config {
    if path.exists() {
        return read(path).unwrap_or_default();
    }

    let config = Config::default();
    let _ = save_to_path(path, &config);
    config
}

/// Read a config file, or `None` when it is missing or invalid.
pub fn read(path: &Path) -> Option<Config> {
    let content = fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// Write a config file, creating its directory when missing.
pub fn save_to_path(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(config).map_err(io::Error::other)?;

    fs::write(path, content)
}

/// Interactively edit the configuration for `mode`.
pub fn interactive_config(
    mode: Mode,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<()> {
    interactive_config_at(&config_path(mode), input, output)
}

/// Interactively edit the configuration stored at `path`.
///
/// Reads answers line by line, so the whole dialog can be exercised in tests.
/// End of input discards the changes instead of looping forever.
pub fn interactive_config_at(
    path: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut config = read(path).unwrap_or_default();

    writeln!(output, "\n=== Headset Control Configuration ===\n")?;

    loop {
        write!(
            output,
            "Default sidetone level (0-128): [{}] ",
            config.default_sidetone
        )?;
        output.flush()?;

        let Some(line) = read_line(input)? else {
            writeln!(output, "\nInput closed, changes discarded")?;
            return Ok(());
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }

        match trimmed.parse::<u8>() {
            Ok(level) if level <= SIDETONE_MAX => {
                config.default_sidetone = level;
                break;
            }
            Ok(_) => writeln!(output, "Please enter a value between 0 and 128")?,
            Err(_) => writeln!(output, "Invalid input")?,
        }
    }

    writeln!(output, "\n=== Key Bindings ===\n")?;

    loop {
        for field in KeyField::ALL {
            write!(
                output,
                "{} ({}): [{}] ",
                field.label(),
                field.name(),
                field.get(&config.keys)
            )?;
            output.flush()?;

            let Some(line) = read_line(input)? else {
                writeln!(output, "\nInput closed, changes discarded")?;
                return Ok(());
            };

            let trimmed = line.trim();
            if let Some(character) = trimmed.chars().next() {
                field.set(&mut config.keys, normalise_binding(character));
            }
        }

        match config.keys.conflict() {
            Some(conflict) => {
                writeln!(output, "\nInvalid keybindings: {conflict}")?;
                writeln!(
                    output,
                    "Please enter a different key for the conflicting binding (Ctrl+D discards)."
                )?;
            }
            None => break,
        }
    }

    writeln!(output, "\nSave changes? (w - write, esc - discard)")?;

    loop {
        write!(output, "> ")?;
        output.flush()?;

        let Some(line) = read_line(input)? else {
            writeln!(output, "Changes discarded")?;
            return Ok(());
        };

        match line.trim().to_lowercase().as_str() {
            "w" => {
                save_to_path(path, &config)?;
                writeln!(output, "Configuration saved to {}", path.display())?;
                return Ok(());
            }
            "" | "esc" => {
                writeln!(output, "Changes discarded")?;
                return Ok(());
            }
            _ => {}
        }
    }
}

/// Read one line; `None` signals end of input.
fn read_line(input: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut line = String::new();

    if input.read_line(&mut line)? == 0 {
        Ok(None)
    } else {
        Ok(Some(line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Mode;
    use std::fs;
    use std::io::Cursor;
    use tempfile::tempdir;

    /// One empty answer for each of the nine key prompts.
    const EMPTY_KEYS: [&str; 9] = [""; 9];

    const FULL_TOML: &str = "\
default_sidetone = 37

[keys]
dec1 = \"z\"
dec10 = \"y\"
inc1 = \"x\"
inc10 = \"w\"
full = \"v\"
none = \"u\"
config = \"t\"
verbose = \"s\"
quit = \"r\"
";

    fn custom_keys() -> KeyConfig {
        KeyConfig {
            dec1: 'z',
            dec10: 'y',
            inc1: 'x',
            inc10: 'w',
            full: 'v',
            none: 'u',
            config: 't',
            verbose: 's',
            quit: 'r',
        }
    }

    /// Feed `lines` to the dialog as newline-terminated answers.
    fn dialog_input(lines: &[&str]) -> Vec<u8> {
        let mut input = String::new();

        for line in lines {
            input.push_str(line);
            input.push('\n');
        }

        input.into_bytes()
    }

    /// Run the dialog and return everything it printed.
    fn run_dialog(path: &Path, lines: &[&str]) -> String {
        let mut input = Cursor::new(dialog_input(lines));
        let mut output = Vec::new();

        interactive_config_at(path, &mut input, &mut output).expect("dialog should return Ok");

        String::from_utf8(output).expect("dialog output is UTF-8")
    }

    /// Answers that keep every current value and then write the file.
    fn keep_all_then<'a>(sidetone: &'a str, save: &'a str) -> Vec<&'a str> {
        let mut lines = vec![sidetone];
        lines.extend(EMPTY_KEYS);
        lines.push(save);
        lines
    }

    // ---------------------------------------------------------------- paths

    #[test]
    fn config_path_in_normal_mode_uses_plain_file_name() {
        let dir = tempdir().unwrap();
        let path = config_path_in(dir.path(), Mode::Normal);

        assert_eq!(path.file_name().unwrap(), "config.toml");
        assert_eq!(path, dir.path().join("config.toml"));
    }

    #[test]
    fn config_path_in_test_mode_uses_test_suffix() {
        let dir = tempdir().unwrap();
        let path = config_path_in(dir.path(), Mode::Test);

        assert_eq!(path.file_name().unwrap(), "config.test.toml");
        assert_eq!(path, dir.path().join("config.test.toml"));
    }

    #[test]
    fn config_path_in_stays_inside_given_directory() {
        let dir = tempdir().unwrap();

        for mode in [Mode::Normal, Mode::Test] {
            let path = config_path_in(dir.path(), mode);

            assert!(path.starts_with(dir.path()));
            assert_eq!(path.parent(), Some(dir.path()));
        }
    }

    #[test]
    fn state_path_in_normal_mode_uses_plain_file_name() {
        let dir = tempdir().unwrap();
        let path = state_path_in(dir.path(), Mode::Normal);

        assert_eq!(path.file_name().unwrap(), "headset-tui-sidetone-state");
        assert_eq!(path, dir.path().join("headset-tui-sidetone-state"));
    }

    #[test]
    fn state_path_in_test_mode_uses_test_suffix() {
        let dir = tempdir().unwrap();
        let path = state_path_in(dir.path(), Mode::Test);

        assert_eq!(path.file_name().unwrap(), "headset-tui-sidetone-state.test");
        assert_eq!(path, dir.path().join("headset-tui-sidetone-state.test"));
    }

    #[test]
    fn state_path_in_stays_inside_given_directory() {
        let dir = tempdir().unwrap();

        for mode in [Mode::Normal, Mode::Test] {
            let path = state_path_in(dir.path(), mode);

            assert!(path.starts_with(dir.path()));
            assert_eq!(path.parent(), Some(dir.path()));
        }
    }

    #[test]
    fn level_path_in_normal_mode_uses_plain_file_name() {
        let dir = tempdir().unwrap();
        let path = level_path_in(dir.path(), Mode::Normal);

        assert_eq!(path.file_name().unwrap(), "headset-tui-sidetone-level");
        assert_eq!(path, dir.path().join("headset-tui-sidetone-level"));
    }

    #[test]
    fn level_path_in_test_mode_uses_test_suffix() {
        let dir = tempdir().unwrap();
        let path = level_path_in(dir.path(), Mode::Test);

        assert_eq!(path.file_name().unwrap(), "headset-tui-sidetone-level.test");
        assert_eq!(path, dir.path().join("headset-tui-sidetone-level.test"));
    }

    #[test]
    fn level_path_in_stays_inside_given_directory() {
        let dir = tempdir().unwrap();

        for mode in [Mode::Normal, Mode::Test] {
            let path = level_path_in(dir.path(), mode);

            assert!(path.starts_with(dir.path()));
            assert_eq!(path.parent(), Some(dir.path()));
        }
    }

    #[test]
    fn normal_and_test_paths_are_distinct() {
        let dir = tempdir().unwrap();

        assert_ne!(
            config_path_in(dir.path(), Mode::Normal),
            config_path_in(dir.path(), Mode::Test)
        );
        assert_ne!(
            state_path_in(dir.path(), Mode::Normal),
            state_path_in(dir.path(), Mode::Test)
        );
        assert_ne!(
            level_path_in(dir.path(), Mode::Normal),
            level_path_in(dir.path(), Mode::Test)
        );
    }

    // ------------------------------------------------------------- defaults

    #[test]
    fn key_config_default_has_expected_bindings() {
        let keys = KeyConfig::default();

        assert_eq!(keys.dec1, 'a');
        assert_eq!(keys.dec10, 's');
        assert_eq!(keys.inc1, 'd');
        assert_eq!(keys.inc10, 'w');
        assert_eq!(keys.full, 'f');
        assert_eq!(keys.none, 'e');
        assert_eq!(keys.quit, 'q');
        assert_eq!(keys.config, 'c');
        assert_eq!(keys.verbose, 'v');
    }

    #[test]
    fn config_default_uses_default_keys_and_zero_sidetone() {
        let config = Config::default();

        assert_eq!(config.keys, KeyConfig::default());
        assert_eq!(config.default_sidetone, 0);
    }

    // ------------------------------------------------------------ key fields

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
    fn key_field_labels_and_names_are_stable() {
        for (field, label, name) in [
            (KeyField::Dec1, "Decrease sidetone by 1", "dec1"),
            (KeyField::Dec10, "Decrease sidetone by 10", "dec10"),
            (KeyField::Inc1, "Increase sidetone by 1", "inc1"),
            (KeyField::Inc10, "Increase sidetone by 10", "inc10"),
            (KeyField::Full, "Set sidetone to full (128)", "full"),
            (KeyField::None, "Set sidetone to none (0)", "none"),
            (KeyField::Config, "Open configuration menu", "config"),
            (KeyField::Verbose, "Toggle verbose mode", "verbose"),
            (KeyField::Quit, "Quit the application", "quit"),
        ] {
            assert_eq!(field.label(), label, "{field:?}");
            assert_eq!(field.name(), name, "{field:?}");
        }
    }

    // ------------------------------------------------------- key conflicts

    #[test]
    fn normalise_binding_lowercases() {
        assert_eq!(normalise_binding('A'), 'a');
        assert_eq!(normalise_binding('z'), 'z');
        assert_eq!(normalise_binding('Ä'), 'ä');
    }

    #[test]
    fn default_bindings_have_no_conflict() {
        assert_eq!(KeyConfig::default().conflict(), None);
    }

    #[test]
    fn duplicate_bindings_are_detected() {
        let keys = KeyConfig {
            dec10: 'a',
            ..KeyConfig::default()
        };

        assert_eq!(
            keys.conflict(),
            Some(KeyConflict::Duplicate {
                field: KeyField::Dec10,
                key: 'a',
                other: KeyField::Dec1,
            })
        );
    }

    #[test]
    fn uppercase_duplicates_are_detected() {
        let keys = KeyConfig {
            dec1: 'A',
            dec10: 'a',
            ..KeyConfig::default()
        };

        assert_eq!(
            keys.conflict(),
            Some(KeyConflict::Duplicate {
                field: KeyField::Dec10,
                key: 'a',
                other: KeyField::Dec1,
            })
        );
    }

    #[test]
    fn reserved_bindings_are_detected() {
        for key in RESERVED_BINDINGS {
            let keys = KeyConfig {
                quit: key,
                ..KeyConfig::default()
            };

            assert_eq!(
                keys.conflict(),
                Some(KeyConflict::Reserved {
                    field: KeyField::Quit,
                    key,
                }),
                "reserved key {key:?}"
            );
        }
    }

    #[test]
    fn binding_available_accepts_an_unused_key() {
        assert_eq!(
            KeyConfig::default().binding_available(KeyField::Dec1, 'z'),
            Ok(())
        );
    }

    #[test]
    fn binding_available_rejects_another_fields_key() {
        assert_eq!(
            KeyConfig::default().binding_available(KeyField::Quit, 'a'),
            Err(KeyConflict::Duplicate {
                field: KeyField::Quit,
                key: 'a',
                other: KeyField::Dec1,
            })
        );
    }

    #[test]
    fn binding_available_rejects_reserved_keys() {
        for key in RESERVED_BINDINGS {
            assert_eq!(
                KeyConfig::default().binding_available(KeyField::Dec1, key),
                Err(KeyConflict::Reserved {
                    field: KeyField::Dec1,
                    key,
                }),
                "reserved key {key:?}"
            );
        }
    }

    #[test]
    fn binding_available_allows_the_fields_own_key() {
        let keys = KeyConfig::default();

        assert_eq!(keys.binding_available(KeyField::Dec1, 'a'), Ok(()));
        assert_eq!(keys.binding_available(KeyField::Dec1, 'A'), Ok(()));
    }

    #[test]
    fn conflict_display_messages_are_stable() {
        assert_eq!(
            KeyConflict::Reserved {
                field: KeyField::Quit,
                key: 'j',
            }
            .to_string(),
            "quit: 'j' is reserved for sidetone control"
        );
        assert_eq!(
            KeyConflict::Duplicate {
                field: KeyField::Quit,
                key: 'a',
                other: KeyField::Dec1,
            }
            .to_string(),
            "quit: 'a' is already bound to dec1"
        );
    }

    // ---------------------------------------------------------------- reads

    #[test]
    fn read_returns_none_for_missing_file() {
        let dir = tempdir().unwrap();

        assert_eq!(read(&dir.path().join("absent.toml")), None);
    }

    #[test]
    fn read_returns_none_for_invalid_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "default_sidetone = \n").unwrap();

        assert_eq!(read(&path), None);
    }

    #[test]
    fn read_returns_none_for_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::create_dir(&path).unwrap();

        assert_eq!(read(&path), None);
    }

    #[test]
    fn read_parses_all_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, FULL_TOML).unwrap();

        assert_eq!(
            read(&path),
            Some(Config {
                keys: custom_keys(),
                default_sidetone: 37,
            })
        );
    }

    #[test]
    fn read_defaults_missing_sidetone_to_zero() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[keys]\ndec1 = \"z\"\ndec10 = \"y\"\ninc1 = \"x\"\ninc10 = \"w\"\n\
             full = \"v\"\nnone = \"u\"\nconfig = \"t\"\nverbose = \"s\"\nquit = \"r\"\n",
        )
        .unwrap();

        let config = read(&path).expect("valid config");

        assert_eq!(config.default_sidetone, 0);
        assert_eq!(config.keys, custom_keys());
    }

    #[test]
    fn read_tolerates_unknown_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "default_sidetone = 12\nfuture_option = \"ignored\"\n\n\
             [keys]\ndec1 = \"a\"\ndec10 = \"s\"\ninc1 = \"d\"\ninc10 = \"w\"\n\
             full = \"f\"\nnone = \"e\"\nconfig = \"c\"\nverbose = \"v\"\nquit = \"q\"\n\
             future_key = \"nope\"\n",
        )
        .unwrap();

        let config = read(&path).expect("unknown fields should be tolerated");

        assert_eq!(config.default_sidetone, 12);
        assert_eq!(config.keys, KeyConfig::default());
    }

    // ---------------------------------------------------------------- saves

    #[test]
    fn save_and_read_roundtrip_preserves_values() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config {
            keys: custom_keys(),
            default_sidetone: 77,
        };

        save_to_path(&path, &config).unwrap();

        assert_eq!(read(&path), Some(config));
    }

    #[test]
    fn save_creates_nested_parent_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/deeper/config.toml");
        let config = Config {
            keys: KeyConfig::default(),
            default_sidetone: 3,
        };

        assert!(!path.parent().unwrap().exists());

        save_to_path(&path, &config).unwrap();

        assert!(path.exists());
        assert!(path.parent().unwrap().is_dir());
        assert_eq!(read(&path), Some(config));
    }

    #[test]
    fn save_writes_valid_toml_with_expected_sections() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config {
            keys: custom_keys(),
            default_sidetone: 21,
        };

        save_to_path(&path, &config).unwrap();

        let content = fs::read_to_string(&path).unwrap();

        assert!(content.contains("[keys]"));
        assert!(content.contains("default_sidetone"));
        assert_eq!(toml::from_str::<Config>(&content).unwrap(), config);
    }

    #[test]
    fn save_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this is not valid toml = \n").unwrap();

        let config = Config {
            keys: KeyConfig::default(),
            default_sidetone: 10,
        };

        save_to_path(&path, &config).unwrap();

        assert_eq!(read(&path), Some(config));
    }

    // --------------------------------------------------------- load_or_create

    #[test]
    fn load_or_create_writes_defaults_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sub/config.toml");

        assert!(!path.exists());

        let config = load_or_create(&path);

        assert_eq!(config, Config::default());
        assert!(path.exists());
        assert_eq!(read(&path), Some(Config::default()));
    }

    #[test]
    fn load_or_create_keeps_existing_config() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let stored = Config {
            keys: custom_keys(),
            default_sidetone: 55,
        };
        save_to_path(&path, &stored).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        let loaded = load_or_create(&path);

        assert_eq!(loaded, stored);
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn load_or_create_keeps_an_invalid_config_on_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let broken = "default_sidetone = \"not a number\"\n";
        fs::write(&path, broken).unwrap();

        let loaded = load_or_create(&path);

        // Defaults are used for this run, but the broken file is preserved so a
        // typo never destroys the user's own bindings.
        assert_eq!(loaded, Config::default());
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
        assert_eq!(read(&path), None);
    }

    #[test]
    fn load_or_create_keeps_a_config_that_is_not_utf8() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let broken = [0xff_u8, 0xfe, 0x00, 0x01];
        fs::write(&path, broken).unwrap();

        let loaded = load_or_create(&path);

        assert_eq!(loaded, Config::default());
        assert_eq!(fs::read(&path).unwrap(), broken);
    }

    // -------------------------------------------------------------- dialogs

    #[test]
    fn dialog_saves_sidetone_and_keys_on_w() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let output = run_dialog(&path, &keep_all_then("64", "w"));

        let config = read(&path).expect("dialog should write the file");
        assert_eq!(config.default_sidetone, 64);
        assert_eq!(config.keys, KeyConfig::default());
        assert!(output.contains(&format!("Configuration saved to {}", path.display())));
    }

    #[test]
    fn dialog_rejects_non_numeric_then_accepts_valid_sidetone() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut lines = vec!["not-a-number", "64"];
        lines.extend(EMPTY_KEYS);
        lines.push("w");

        let output = run_dialog(&path, &lines);

        assert!(output.contains("Invalid input"));
        assert_eq!(read(&path).unwrap().default_sidetone, 64);
    }

    #[test]
    fn dialog_rejects_out_of_range_then_accepts_valid_sidetone() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut lines = vec!["129", "64"];
        lines.extend(EMPTY_KEYS);
        lines.push("w");

        let output = run_dialog(&path, &lines);

        assert!(output.contains("Please enter a value between 0 and 128"));
        assert_eq!(read(&path).unwrap().default_sidetone, 64);
    }

    #[test]
    fn dialog_empty_sidetone_keeps_existing_value() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let stored = Config {
            keys: KeyConfig::default(),
            default_sidetone: 42,
        };
        save_to_path(&path, &stored).unwrap();

        run_dialog(&path, &keep_all_then("", "w"));

        assert_eq!(read(&path).unwrap().default_sidetone, 42);
    }

    #[test]
    fn dialog_rebinds_keys_using_first_lowercased_character() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let lines = ["50", "A", "B", "", "Xyz", "F", "", "C", "V", "Q", "w"];

        let output = run_dialog(&path, &lines);

        let config = read(&path).expect("dialog should write the file");
        assert_eq!(config.default_sidetone, 50);
        assert_eq!(
            config.keys,
            KeyConfig {
                dec1: 'a',
                dec10: 'b',
                inc1: 'd',
                inc10: 'x',
                full: 'f',
                none: 'e',
                config: 'c',
                verbose: 'v',
                quit: 'q',
            }
        );
        assert!(!output.contains("Changes discarded"));
    }

    #[test]
    fn dialog_trims_whitespace_around_answers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let lines = [
            " 64 ", " Z ", " y ", " x ", " w ", " v ", " u ", " t ", " s ", " r ", " W ",
        ];

        run_dialog(&path, &lines);

        let config = read(&path).expect("dialog should write the file");
        assert_eq!(config.default_sidetone, 64);
        assert_eq!(config.keys, custom_keys());
    }

    #[test]
    fn dialog_empty_line_discards_changes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let output = run_dialog(&path, &keep_all_then("64", ""));

        assert!(!path.exists());
        assert!(output.contains("Changes discarded"));
    }

    #[test]
    fn dialog_esc_discards_changes_case_insensitively() {
        for answer in ["esc", "ESC", "Esc"] {
            let dir = tempdir().unwrap();
            let path = dir.path().join("config.toml");

            let output = run_dialog(&path, &keep_all_then("64", answer));

            assert!(!path.exists(), "{answer:?} should discard");
            assert!(output.contains("Changes discarded"));
        }
    }

    #[test]
    fn dialog_discard_leaves_existing_file_unchanged() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let stored = Config {
            keys: custom_keys(),
            default_sidetone: 42,
        };
        save_to_path(&path, &stored).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        let mut lines = vec!["10"];
        lines.extend(["b", "c", "d", "e", "g", "i", "m", "n", "o"]);
        lines.push("esc");
        run_dialog(&path, &lines);

        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert_eq!(read(&path), Some(stored));
    }

    #[test]
    fn dialog_rejects_duplicate_bindings_and_reprompts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut lines = vec!["50"];

        // First pass: dec1 collides with the default dec10 binding.
        lines.extend(["s", "", "", "", "", "", "", "", ""]);
        // Second pass: move dec1 to a free key.
        lines.extend(["b", "", "", "", "", "", "", "", ""]);
        lines.push("w");

        let output = run_dialog(&path, &lines);

        assert!(output.contains("Invalid keybindings: dec10: 's' is already bound to dec1"));
        let config = read(&path).expect("dialog should write the file");
        assert_eq!(config.keys.dec1, 'b');
        assert_eq!(config.keys.dec10, 's');
    }

    #[test]
    fn dialog_rejects_reserved_bindings_and_reprompts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut lines = vec!["50"];

        // First pass: h is reserved for the fixed sidetone shortcut.
        lines.extend(["h", "", "", "", "", "", "", "", ""]);
        // Second pass: use a free key.
        lines.extend(["b", "", "", "", "", "", "", "", ""]);
        lines.push("w");

        let output = run_dialog(&path, &lines);

        assert!(output.contains("Invalid keybindings: dec1: 'h' is reserved for sidetone control"));
        assert_eq!(read(&path).unwrap().keys.dec1, 'b');
    }

    #[test]
    fn dialog_allows_swapping_two_bindings_in_one_pass() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut lines = vec!["50"];
        lines.extend(["s", "a", "", "", "", "", "", "", ""]);
        lines.push("w");

        run_dialog(&path, &lines);

        let config = read(&path).expect("dialog should write the file");
        assert_eq!(config.keys.dec1, 's');
        assert_eq!(config.keys.dec10, 'a');
    }

    #[test]
    fn dialog_unknown_save_answer_then_writes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut lines = vec!["64"];
        lines.extend(EMPTY_KEYS);
        lines.extend(["maybe", "w"]);

        let output = run_dialog(&path, &lines);

        assert!(path.exists());
        assert!(output.contains("Configuration saved to"));
        assert_eq!(read(&path).unwrap().default_sidetone, 64);
    }

    #[test]
    fn dialog_eof_at_sidetone_prompt_discards() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let output = run_dialog(&path, &[]);

        assert!(!path.exists());
        assert!(output.contains("Input closed, changes discarded"));
    }

    #[test]
    fn dialog_eof_at_key_prompt_discards() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let output = run_dialog(&path, &["64"]);

        assert!(!path.exists());
        assert!(output.contains("Input closed, changes discarded"));
    }

    #[test]
    fn dialog_eof_at_save_prompt_discards() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut lines = vec!["64"];
        lines.extend(EMPTY_KEYS);

        let output = run_dialog(&path, &lines);

        assert!(!path.exists());
        assert!(output.contains("Changes discarded"));
    }

    #[test]
    fn dialog_output_contains_prompts_and_sections() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let output = run_dialog(&path, &keep_all_then("64", "w"));

        for expected in [
            "=== Headset Control Configuration ===",
            "Default sidetone level (0-128):",
            "=== Key Bindings ===",
            "Decrease sidetone by 1 (dec1):",
            "Decrease sidetone by 10 (dec10):",
            "Increase sidetone by 1 (inc1):",
            "Increase sidetone by 10 (inc10):",
            "Set sidetone to full (128) (full):",
            "Set sidetone to none (0) (none):",
            "Open configuration menu (config):",
            "Toggle verbose mode (verbose):",
            "Quit the application (quit):",
            "Save changes? (w - write, esc - discard)",
            "> ",
            "Configuration saved to",
        ] {
            assert!(output.contains(expected), "missing prompt: {expected:?}");
        }
    }

    #[test]
    fn dialog_uses_existing_config_as_starting_values() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let stored = Config {
            keys: KeyConfig {
                dec1: 'z',
                ..KeyConfig::default()
            },
            default_sidetone: 42,
        };
        save_to_path(&path, &stored).unwrap();

        let output = run_dialog(&path, &keep_all_then("", "esc"));

        assert!(output.contains("Default sidetone level (0-128): [42]"));
        assert!(output.contains("Decrease sidetone by 1 (dec1): [z]"));
        assert!(output.contains("Decrease sidetone by 10 (dec10): [s]"));
    }
}
