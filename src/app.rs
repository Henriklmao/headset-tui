//! Application state and the actions that mutate it.

use crate::cli::Mode;
use crate::headset::{BatteryState, Headset, HeadsetError, HeadsetStatus, SIDETONE_MAX};
use crate::input::Action;
use std::io;
use std::path::Path;

/// State rendered by the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiState {
    pub sidetone: u8,
    /// Battery percentage, or `None` when it cannot be read.
    pub battery: Option<i32>,
    pub battery_state: BatteryState,
    pub device_name: String,
    pub has_sidetone: bool,
    pub verbose_mode: bool,
    pub mode: Mode,
    /// Last failure reported by the headset backend, if any.
    pub last_error: Option<String>,
}

impl UiState {
    pub fn new(sidetone: u8, mode: Mode) -> Self {
        Self {
            sidetone: sidetone.min(SIDETONE_MAX),
            battery: None,
            battery_state: BatteryState::Unavailable,
            device_name: String::new(),
            has_sidetone: true,
            verbose_mode: false,
            mode,
            last_error: None,
        }
    }
}

/// What the event loop should do after an action was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Quit,
    OpenConfig,
}

/// Copy a status (or its absence) into the UI state.
pub fn apply_status(state: &mut UiState, result: Result<HeadsetStatus, HeadsetError>) {
    match result {
        Ok(status) => {
            state.device_name = status.device_name;
            state.battery = Some(status.battery_level);
            state.battery_state = status.battery_state;
            state.has_sidetone = status.has_sidetone;

            // Only devices that can report the level update the gauge; on all
            // others the level last set through this app stays visible.
            if let Some(level) = status.sidetone_level {
                state.sidetone = level.min(SIDETONE_MAX);
            }
        }
        Err(_) => {
            state.battery = None;
            state.battery_state = BatteryState::Unavailable;
            state.device_name = "Unknown Device".to_string();
            state.has_sidetone = false;
        }
    }
}

/// Apply one key action and report what the event loop should do next.
///
/// The sidetone shown in the UI always follows the key press, even when the
/// headset rejects the write: the gauge is a level selector, not a read-back of
/// the device (HeadsetControl cannot read sidetone on all devices). A rejected
/// write is stored in [`UiState::last_error`] so the TUI can show it.
pub fn apply_action(action: Action, state: &mut UiState, headset: &dyn Headset) -> Outcome {
    match action {
        Action::DecBy1 => shift(state, headset, -1),
        Action::DecBy10 => shift(state, headset, -10),
        Action::IncBy1 => shift(state, headset, 1),
        Action::IncBy10 => shift(state, headset, 10),
        Action::SetFull => set_level(state, headset, SIDETONE_MAX),
        Action::SetNone => set_level(state, headset, 0),
        Action::ToggleVerbose => state.verbose_mode = !state.verbose_mode,
        Action::Config => return Outcome::OpenConfig,
        Action::Quit => return Outcome::Quit,
        Action::None => {}
    }

    Outcome::Continue
}

fn shift(state: &mut UiState, headset: &dyn Headset, delta: i16) {
    let level = (i16::from(state.sidetone) + delta).clamp(0, i16::from(SIDETONE_MAX)) as u8;
    set_level(state, headset, level);
}

fn set_level(state: &mut UiState, headset: &dyn Headset, level: u8) {
    if state.sidetone == level {
        return;
    }

    match headset.set_sidetone(level) {
        Ok(()) => state.last_error = None,
        Err(err) => state.last_error = Some(err.to_string()),
    }

    state.sidetone = level;
}

/// Pick the level shown when the TUI starts.
///
/// A level reported by the device wins (it may have been changed elsewhere),
/// then the level this app set last, then the configured default.
pub fn initial_level(reported: Option<u8>, saved: Option<u8>, default: u8) -> u8 {
    reported.or(saved).unwrap_or(default).min(SIDETONE_MAX)
}

/// Read the sidetone level last saved by this app.
pub fn saved_level(path: &Path) -> Option<u8> {
    let text = std::fs::read_to_string(path).ok()?;
    let level = text.trim().parse::<u16>().ok()?;

    Some(level.min(u16::from(SIDETONE_MAX)) as u8)
}

/// Persist the sidetone level this app last set, creating its directory when
/// missing.
pub fn save_level(path: &Path, level: u8) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, level.min(SIDETONE_MAX).to_string())
}

/// Read whether sidetone is currently toggled on.
pub fn toggle_state(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|value| value.trim() == "on")
        .unwrap_or(false)
}

/// Toggle sidetone between off and `default_sidetone`.
///
/// Returns the new on/off state. A `default_sidetone` of 0 means "no default
/// configured", so nothing is changed and the previous state is returned.
pub fn toggle_sidetone(
    headset: &dyn Headset,
    default_sidetone: u8,
    state_path: &Path,
) -> io::Result<bool> {
    let was_on = toggle_state(state_path);

    if default_sidetone == 0 {
        return Ok(was_on);
    }

    let level = if was_on {
        0
    } else {
        default_sidetone.min(SIDETONE_MAX)
    };
    headset
        .set_sidetone(level)
        .map_err(|err| io::Error::other(err.to_string()))?;

    let now_on = level > 0;
    save_toggle_state(state_path, now_on)?;
    Ok(now_on)
}

/// Persist the toggle state, creating its directory when missing.
pub fn save_toggle_state(path: &Path, on: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, if on { "on" } else { "off" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headset::mock::MockHeadset;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).expect("read state file")
    }

    fn status(has_sidetone: bool) -> HeadsetStatus {
        HeadsetStatus {
            device_name: "HyperX Cloud".to_string(),
            battery_level: 73,
            battery_state: BatteryState::Good,
            has_sidetone,
            sidetone_level: None,
        }
    }

    fn status_with_sidetone(level: u8) -> HeadsetStatus {
        HeadsetStatus {
            sidetone_level: Some(level),
            ..status(true)
        }
    }

    fn ui_state(sidetone: u8) -> UiState {
        UiState::new(sidetone, Mode::Normal)
    }

    // -- UiState::new ------------------------------------------------------

    #[test]
    fn new_state_has_expected_defaults() {
        let state = UiState::new(37, Mode::Normal);

        assert_eq!(state.sidetone, 37);
        assert_eq!(state.battery, None);
        assert_eq!(state.battery_state, BatteryState::Unavailable);
        assert_eq!(state.device_name, "");
        assert!(state.has_sidetone);
        assert!(!state.verbose_mode);
        assert_eq!(state.mode, Mode::Normal);
        assert_eq!(state.last_error, None);
    }

    #[test]
    fn new_state_clamps_sidetone_above_max() {
        assert_eq!(UiState::new(200, Mode::Normal).sidetone, SIDETONE_MAX);
        assert_eq!(UiState::new(129, Mode::Normal).sidetone, SIDETONE_MAX);
    }

    #[test]
    fn new_state_keeps_levels_at_or_below_max() {
        assert_eq!(UiState::new(128, Mode::Normal).sidetone, 128);
        assert_eq!(UiState::new(64, Mode::Normal).sidetone, 64);
        assert_eq!(UiState::new(0, Mode::Normal).sidetone, 0);
    }

    #[test]
    fn new_state_stores_mode() {
        assert_eq!(UiState::new(10, Mode::Test).mode, Mode::Test);
    }

    // -- apply_status ------------------------------------------------------

    #[test]
    fn apply_status_copies_device_name() {
        let mut state = ui_state(10);

        apply_status(&mut state, Ok(status(true)));

        assert_eq!(state.device_name, "HyperX Cloud");
    }

    #[test]
    fn apply_status_copies_battery_level_and_state() {
        let mut state = ui_state(10);

        apply_status(&mut state, Ok(status(true)));

        assert_eq!(state.battery, Some(73));
        assert_eq!(state.battery_state, BatteryState::Good);
    }

    #[test]
    fn apply_status_copies_sidetone_capability_true() {
        let mut state = ui_state(10);

        apply_status(&mut state, Ok(status(true)));

        assert!(state.has_sidetone);
    }

    #[test]
    fn apply_status_copies_sidetone_capability_false() {
        let mut state = ui_state(10);

        apply_status(&mut state, Ok(status(false)));

        assert!(!state.has_sidetone);
    }

    #[test]
    fn apply_status_uses_a_level_reported_by_the_device() {
        let mut state = ui_state(10);

        apply_status(&mut state, Ok(status_with_sidetone(72)));

        assert_eq!(state.sidetone, 72);
    }

    #[test]
    fn apply_status_keeps_the_local_level_when_the_device_cannot_report_it() {
        let mut state = ui_state(10);

        apply_status(&mut state, Ok(status(true)));

        assert_eq!(state.sidetone, 10);
    }

    #[test]
    fn apply_status_clamps_a_reported_level() {
        let mut state = ui_state(10);

        apply_status(&mut state, Ok(status_with_sidetone(200)));

        assert_eq!(state.sidetone, SIDETONE_MAX);
    }

    #[test]
    fn apply_status_error_reports_unknown_device() {
        let mut state = ui_state(10);

        apply_status(&mut state, Err(HeadsetError::NoDevice));

        assert_eq!(state.device_name, "Unknown Device");
    }

    #[test]
    fn apply_status_error_clears_battery() {
        let mut state = ui_state(10);

        apply_status(&mut state, Err(HeadsetError::NoDevice));

        assert_eq!(state.battery, None);
        assert_eq!(state.battery_state, BatteryState::Unavailable);
    }

    #[test]
    fn apply_status_error_disables_sidetone() {
        let mut state = ui_state(10);

        apply_status(&mut state, Err(HeadsetError::CommandFailed(1)));

        assert!(!state.has_sidetone);
    }

    #[test]
    fn apply_status_error_keeps_sidetone_level_and_mode() {
        let mut state = UiState::new(55, Mode::Test);

        apply_status(&mut state, Err(HeadsetError::Parse("oops".to_string())));

        assert_eq!(state.sidetone, 55);
        assert_eq!(state.mode, Mode::Test);
    }

    #[test]
    fn apply_status_success_after_error_recovers() {
        let mut state = ui_state(10);
        apply_status(&mut state, Err(HeadsetError::NoDevice));

        apply_status(&mut state, Ok(status(true)));

        assert_eq!(state.device_name, "HyperX Cloud");
        assert_eq!(state.battery, Some(73));
        assert_eq!(state.battery_state, BatteryState::Good);
        assert!(state.has_sidetone);
    }

    #[test]
    fn apply_status_error_after_success_clears_fields() {
        let mut state = ui_state(10);
        apply_status(&mut state, Ok(status(true)));

        apply_status(
            &mut state,
            Err(HeadsetError::Unavailable("gone".to_string())),
        );

        assert_eq!(state.battery, None);
        assert_eq!(state.battery_state, BatteryState::Unavailable);
        assert_eq!(state.device_name, "Unknown Device");
        assert!(!state.has_sidetone);
    }

    // -- apply_action: level changes ---------------------------------------

    #[test]
    fn dec_by_1_lowers_level() {
        let headset = MockHeadset::available();
        let mut state = ui_state(10);

        let outcome = apply_action(Action::DecBy1, &mut state, &headset);

        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(state.sidetone, 9);
        assert_eq!(headset.calls(), vec![9]);
    }

    #[test]
    fn dec_by_10_lowers_level() {
        let headset = MockHeadset::available();
        let mut state = ui_state(60);

        apply_action(Action::DecBy10, &mut state, &headset);

        assert_eq!(state.sidetone, 50);
        assert_eq!(headset.calls(), vec![50]);
    }

    #[test]
    fn inc_by_1_raises_level() {
        let headset = MockHeadset::available();
        let mut state = ui_state(10);

        apply_action(Action::IncBy1, &mut state, &headset);

        assert_eq!(state.sidetone, 11);
        assert_eq!(headset.calls(), vec![11]);
    }

    #[test]
    fn inc_by_10_raises_level() {
        let headset = MockHeadset::available();
        let mut state = ui_state(20);

        apply_action(Action::IncBy10, &mut state, &headset);

        assert_eq!(state.sidetone, 30);
        assert_eq!(headset.calls(), vec![30]);
    }

    #[test]
    fn dec_by_10_from_five_clamps_to_zero() {
        let headset = MockHeadset::available();
        let mut state = ui_state(5);

        apply_action(Action::DecBy10, &mut state, &headset);

        assert_eq!(state.sidetone, 0);
        assert_eq!(headset.calls(), vec![0]);
    }

    #[test]
    fn inc_by_10_from_120_clamps_to_max() {
        let headset = MockHeadset::available();
        let mut state = ui_state(120);

        apply_action(Action::IncBy10, &mut state, &headset);

        assert_eq!(state.sidetone, SIDETONE_MAX);
        assert_eq!(headset.calls(), vec![SIDETONE_MAX]);
    }

    #[test]
    fn inc_by_1_from_127_clamps_to_max() {
        let headset = MockHeadset::available();
        let mut state = ui_state(127);

        apply_action(Action::IncBy1, &mut state, &headset);

        assert_eq!(state.sidetone, 128);
        assert_eq!(headset.calls(), vec![128]);
    }

    #[test]
    fn dec_by_1_at_zero_does_not_call_headset() {
        let headset = MockHeadset::available();
        let mut state = ui_state(0);

        let outcome = apply_action(Action::DecBy1, &mut state, &headset);

        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(state.sidetone, 0);
        assert!(headset.calls().is_empty());
    }

    #[test]
    fn inc_by_1_at_max_does_not_call_headset() {
        let headset = MockHeadset::available();
        let mut state = ui_state(SIDETONE_MAX);

        apply_action(Action::IncBy1, &mut state, &headset);

        assert_eq!(state.sidetone, SIDETONE_MAX);
        assert!(headset.calls().is_empty());
    }

    #[test]
    fn inc_by_10_at_max_does_not_call_headset() {
        let headset = MockHeadset::available();
        let mut state = ui_state(SIDETONE_MAX);

        apply_action(Action::IncBy10, &mut state, &headset);

        assert_eq!(state.sidetone, SIDETONE_MAX);
        assert!(headset.calls().is_empty());
    }

    #[test]
    fn dec_by_10_at_zero_does_not_call_headset() {
        let headset = MockHeadset::available();
        let mut state = ui_state(0);

        apply_action(Action::DecBy10, &mut state, &headset);

        assert_eq!(state.sidetone, 0);
        assert!(headset.calls().is_empty());
    }

    // -- apply_action: absolute values -------------------------------------

    #[test]
    fn set_full_sets_max_level() {
        let headset = MockHeadset::available();
        let mut state = ui_state(12);

        let outcome = apply_action(Action::SetFull, &mut state, &headset);

        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(state.sidetone, SIDETONE_MAX);
        assert_eq!(headset.calls(), vec![SIDETONE_MAX]);
    }

    #[test]
    fn set_full_at_max_does_not_call_headset() {
        let headset = MockHeadset::available();
        let mut state = ui_state(SIDETONE_MAX);

        apply_action(Action::SetFull, &mut state, &headset);

        assert_eq!(state.sidetone, SIDETONE_MAX);
        assert!(headset.calls().is_empty());
    }

    #[test]
    fn set_none_sets_zero() {
        let headset = MockHeadset::available();
        let mut state = ui_state(12);

        let outcome = apply_action(Action::SetNone, &mut state, &headset);

        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(state.sidetone, 0);
        assert_eq!(headset.calls(), vec![0]);
    }

    #[test]
    fn set_none_at_zero_does_not_call_headset() {
        let headset = MockHeadset::available();
        let mut state = ui_state(0);

        apply_action(Action::SetNone, &mut state, &headset);

        assert_eq!(state.sidetone, 0);
        assert!(headset.calls().is_empty());
    }

    // -- apply_action: recording and failure behaviour ---------------------

    #[test]
    fn every_changed_level_is_written_in_order() {
        let headset = MockHeadset::available();
        let mut state = ui_state(50);

        apply_action(Action::IncBy10, &mut state, &headset);
        apply_action(Action::DecBy1, &mut state, &headset);
        apply_action(Action::DecBy10, &mut state, &headset);
        apply_action(Action::SetFull, &mut state, &headset);
        apply_action(Action::SetNone, &mut state, &headset);

        assert_eq!(state.sidetone, 0);
        assert_eq!(headset.calls(), vec![60, 59, 49, 128, 0]);
    }

    #[test]
    fn backend_failure_still_updates_state() {
        let mut headset = MockHeadset::available();
        headset.fail_set = true;
        let mut state = ui_state(10);

        let outcome = apply_action(Action::DecBy1, &mut state, &headset);

        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(state.sidetone, 9);
        assert_eq!(headset.calls(), vec![9]);
        assert_eq!(
            state.last_error.as_deref(),
            Some("headsetcontrol exited with status 1")
        );
    }

    #[test]
    fn backend_failure_still_updates_state_for_absolute_actions() {
        let mut headset = MockHeadset::available();
        headset.fail_set = true;
        let mut state = ui_state(10);

        apply_action(Action::SetFull, &mut state, &headset);

        assert_eq!(state.sidetone, 128);
        assert_eq!(headset.calls(), vec![128]);
        assert!(state.last_error.is_some());
    }

    #[test]
    fn a_successful_write_clears_a_previous_error() {
        let mut headset = MockHeadset::available();
        headset.fail_set = true;
        let mut state = ui_state(10);

        apply_action(Action::IncBy1, &mut state, &headset);
        assert!(state.last_error.is_some());

        headset.fail_set = false;
        apply_action(Action::IncBy1, &mut state, &headset);

        assert_eq!(state.last_error, None);
    }

    #[test]
    fn a_noop_write_keeps_a_previous_error_visible() {
        let headset = MockHeadset::available();
        let mut state = ui_state(0);
        state.last_error = Some("previous failure".to_string());

        apply_action(Action::SetNone, &mut state, &headset);

        // Nothing was written, so the old error is still relevant.
        assert_eq!(state.last_error.as_deref(), Some("previous failure"));
    }

    // -- apply_action: non-level actions -----------------------------------

    #[test]
    fn toggle_verbose_flips_flag_and_continues() {
        let headset = MockHeadset::available();
        let mut state = ui_state(10);

        assert_eq!(
            apply_action(Action::ToggleVerbose, &mut state, &headset),
            Outcome::Continue
        );
        assert!(state.verbose_mode);

        assert_eq!(
            apply_action(Action::ToggleVerbose, &mut state, &headset),
            Outcome::Continue
        );
        assert!(!state.verbose_mode);

        assert!(headset.calls().is_empty());
        assert_eq!(state.sidetone, 10);
    }

    #[test]
    fn config_opens_config_without_backend_calls() {
        let headset = MockHeadset::available();
        let mut state = ui_state(10);

        let outcome = apply_action(Action::Config, &mut state, &headset);

        assert_eq!(outcome, Outcome::OpenConfig);
        assert!(headset.calls().is_empty());
        assert_eq!(state, ui_state(10));
    }

    #[test]
    fn quit_returns_quit_without_backend_calls() {
        let headset = MockHeadset::available();
        let mut state = ui_state(10);

        let outcome = apply_action(Action::Quit, &mut state, &headset);

        assert_eq!(outcome, Outcome::Quit);
        assert!(headset.calls().is_empty());
        assert_eq!(state, ui_state(10));
    }

    #[test]
    fn none_continues_without_changing_state() {
        let headset = MockHeadset::available();
        let mut state = ui_state(42);
        let before = state.clone();

        let outcome = apply_action(Action::None, &mut state, &headset);

        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(state, before);
        assert!(headset.calls().is_empty());
    }

    // -- initial_level -----------------------------------------------------

    #[test]
    fn initial_level_prefers_the_device_reading() {
        assert_eq!(initial_level(Some(30), Some(40), 50), 30);
    }

    #[test]
    fn initial_level_falls_back_to_the_saved_level() {
        assert_eq!(initial_level(None, Some(40), 50), 40);
    }

    #[test]
    fn initial_level_falls_back_to_the_default() {
        assert_eq!(initial_level(None, None, 50), 50);
    }

    #[test]
    fn initial_level_can_be_zero() {
        assert_eq!(initial_level(Some(0), Some(40), 50), 0);
        assert_eq!(initial_level(None, Some(0), 50), 0);
        assert_eq!(initial_level(None, None, 0), 0);
    }

    #[test]
    fn initial_level_clamps_the_default() {
        assert_eq!(initial_level(None, None, 200), SIDETONE_MAX);
    }

    // -- saved_level -------------------------------------------------------

    #[test]
    fn saved_level_missing_file_is_none() {
        let dir = temp_dir();

        assert_eq!(saved_level(&dir.path().join("missing")), None);
    }

    #[test]
    fn saved_level_reads_a_written_level() {
        let dir = temp_dir();
        let path = dir.path().join("level");
        std::fs::write(&path, "72").unwrap();

        assert_eq!(saved_level(&path), Some(72));
    }

    #[test]
    fn saved_level_ignores_surrounding_whitespace() {
        let dir = temp_dir();
        let path = dir.path().join("level");
        std::fs::write(&path, " 72\n").unwrap();

        assert_eq!(saved_level(&path), Some(72));
    }

    #[test]
    fn saved_level_accepts_boundaries() {
        let dir = temp_dir();
        let path = dir.path().join("level");

        std::fs::write(&path, "0").unwrap();
        assert_eq!(saved_level(&path), Some(0));

        std::fs::write(&path, "128").unwrap();
        assert_eq!(saved_level(&path), Some(SIDETONE_MAX));
    }

    #[test]
    fn saved_level_clamps_out_of_range_values() {
        let dir = temp_dir();
        let path = dir.path().join("level");

        std::fs::write(&path, "200").unwrap();
        assert_eq!(saved_level(&path), Some(SIDETONE_MAX));

        std::fs::write(&path, "60000").unwrap();
        assert_eq!(saved_level(&path), Some(SIDETONE_MAX));
    }

    #[test]
    fn saved_level_rejects_garbage() {
        let dir = temp_dir();
        let path = dir.path().join("level");

        for content in ["", "abc", "-1", "1.5", "on"] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(saved_level(&path), None, "{content:?}");
        }
    }

    // -- save_level --------------------------------------------------------

    #[test]
    fn save_level_writes_the_number() {
        let dir = temp_dir();
        let path = dir.path().join("level");

        save_level(&path, 64).unwrap();

        assert_eq!(read(&path), "64");
        assert_eq!(saved_level(&path), Some(64));
    }

    #[test]
    fn save_level_clamps_above_max() {
        let dir = temp_dir();
        let path = dir.path().join("level");

        save_level(&path, 200).unwrap();

        assert_eq!(read(&path), SIDETONE_MAX.to_string());
    }

    #[test]
    fn save_level_creates_nested_directories() {
        let dir = temp_dir();
        let path = dir.path().join("a").join("b").join("level");

        save_level(&path, 12).unwrap();

        assert!(path.exists());
        assert_eq!(saved_level(&path), Some(12));
    }

    #[test]
    fn save_level_overwrites_existing_content() {
        let dir = temp_dir();
        let path = dir.path().join("level");
        std::fs::write(&path, "garbage").unwrap();

        save_level(&path, 0).unwrap();

        assert_eq!(read(&path), "0");
    }

    // -- toggle_state ------------------------------------------------------

    #[test]
    fn toggle_state_missing_file_is_off() {
        let dir = temp_dir();

        assert!(!toggle_state(&dir.path().join("missing")));
    }

    #[test]
    fn toggle_state_on_is_true() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "on").unwrap();

        assert!(toggle_state(&path));
    }

    #[test]
    fn toggle_state_off_is_false() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "off").unwrap();

        assert!(!toggle_state(&path));
    }

    #[test]
    fn toggle_state_ignores_surrounding_whitespace_and_newlines() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "  on\n\t ").unwrap();

        assert!(toggle_state(&path));
    }

    #[test]
    fn toggle_state_unrelated_content_is_false() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "maybe").unwrap();

        assert!(!toggle_state(&path));
    }

    #[test]
    fn toggle_state_empty_file_is_false() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "").unwrap();

        assert!(!toggle_state(&path));
    }

    #[test]
    fn toggle_state_is_case_sensitive() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "ON").unwrap();

        assert!(!toggle_state(&path));
    }

    // -- toggle_sidetone ---------------------------------------------------

    #[test]
    fn toggle_sidetone_without_default_returns_off_when_missing() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        let headset = MockHeadset::available();

        let result = toggle_sidetone(&headset, 0, &path).unwrap();

        assert!(!result);
        assert!(headset.calls().is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn toggle_sidetone_without_default_returns_on_and_keeps_file() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "on").unwrap();
        let headset = MockHeadset::available();

        let result = toggle_sidetone(&headset, 0, &path).unwrap();

        assert!(result);
        assert!(headset.calls().is_empty());
        assert_eq!(read(&path), "on");
    }

    #[test]
    fn toggle_sidetone_when_off_writes_default_and_marks_on() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "off").unwrap();
        let headset = MockHeadset::available();

        let result = toggle_sidetone(&headset, 50, &path).unwrap();

        assert!(result);
        assert_eq!(headset.calls(), vec![50]);
        assert_eq!(read(&path), "on");
    }

    #[test]
    fn toggle_sidetone_when_missing_writes_default_and_marks_on() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        let headset = MockHeadset::available();

        let result = toggle_sidetone(&headset, 64, &path).unwrap();

        assert!(result);
        assert_eq!(headset.calls(), vec![64]);
        assert_eq!(read(&path), "on");
    }

    #[test]
    fn toggle_sidetone_when_on_writes_zero_and_marks_off() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "on").unwrap();
        let headset = MockHeadset::available();

        let result = toggle_sidetone(&headset, 50, &path).unwrap();

        assert!(!result);
        assert_eq!(headset.calls(), vec![0]);
        assert_eq!(read(&path), "off");
    }

    #[test]
    fn toggle_sidetone_twice_returns_to_start() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        let headset = MockHeadset::available();

        assert!(toggle_sidetone(&headset, 60, &path).unwrap());
        assert!(!toggle_sidetone(&headset, 60, &path).unwrap());
        assert!(toggle_sidetone(&headset, 60, &path).unwrap());

        assert_eq!(headset.calls(), vec![60, 0, 60]);
        assert_eq!(read(&path), "on");
    }

    #[test]
    fn toggle_sidetone_clamps_default_above_max() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        let headset = MockHeadset::available();

        toggle_sidetone(&headset, 200, &path).unwrap();

        assert_eq!(headset.calls(), vec![SIDETONE_MAX]);
        assert_eq!(read(&path), "on");
    }

    #[test]
    fn toggle_sidetone_backend_failure_returns_error_for_off() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "off").unwrap();
        let mut headset = MockHeadset::available();
        headset.fail_set = true;

        let error = toggle_sidetone(&headset, 40, &path).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(headset.calls(), vec![40]);
        assert_eq!(read(&path), "off");
    }

    #[test]
    fn toggle_sidetone_backend_failure_keeps_on_state() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "on").unwrap();
        let mut headset = MockHeadset::available();
        headset.fail_set = true;

        assert!(toggle_sidetone(&headset, 40, &path).is_err());
        assert_eq!(read(&path), "on");
    }

    #[test]
    fn toggle_sidetone_creates_missing_parent_directories() {
        let dir = temp_dir();
        let path = dir.path().join("nested").join("deeper").join("state");
        let headset = MockHeadset::available();

        let result = toggle_sidetone(&headset, 30, &path).unwrap();

        assert!(result);
        assert!(path.exists());
        assert_eq!(read(&path), "on");
    }

    // -- save_toggle_state -------------------------------------------------

    #[test]
    fn save_toggle_state_writes_on() {
        let dir = temp_dir();
        let path = dir.path().join("state");

        save_toggle_state(&path, true).unwrap();

        assert_eq!(read(&path), "on");
    }

    #[test]
    fn save_toggle_state_writes_off() {
        let dir = temp_dir();
        let path = dir.path().join("state");

        save_toggle_state(&path, false).unwrap();

        assert_eq!(read(&path), "off");
    }

    #[test]
    fn save_toggle_state_creates_nested_directories() {
        let dir = temp_dir();
        let path = dir.path().join("a").join("b").join("c").join("state");

        save_toggle_state(&path, true).unwrap();

        assert!(path.exists());
        assert_eq!(read(&path), "on");
    }

    #[test]
    fn save_toggle_state_overwrites_existing_content() {
        let dir = temp_dir();
        let path = dir.path().join("state");
        std::fs::write(&path, "on\n").unwrap();

        save_toggle_state(&path, false).unwrap();
        assert_eq!(read(&path), "off");

        save_toggle_state(&path, true).unwrap();
        assert_eq!(read(&path), "on");
    }
}
