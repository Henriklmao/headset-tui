mod app;
mod cli;
mod config;
mod config_editor;
mod headset;
mod input;
mod ui;
mod waybar;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use app::{Outcome, UiState};
use cli::{Cli, Command, Mode};
use config::KeyConfig;
use config_editor::{ConfigEditor, EditorOutcome};
use headset::{Headset, HeadsetControl};
use input::Action;

/// How often the battery is re-read while the TUI is idle.
const STATUS_REFRESH: Duration = Duration::from_secs(2);

fn main() -> ExitCode {
    match Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => match run(cli) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("headset-tui: {err}");
                ExitCode::FAILURE
            }
        },
        Err(err) => {
            eprintln!("headset-tui: {err}");
            eprintln!("Run 'headset-tui --help' for usage.");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Help => {
            println!("{}", cli::help_text());
            Ok(())
        }
        Command::Config => {
            let stdin = io::stdin();
            let mut input = stdin.lock();
            let mut output = io::stdout();
            config::interactive_config(cli.mode, &mut input, &mut output)?;
            Ok(())
        }
        Command::WaybarStatus => {
            print_waybar_status(cli.mode);
            Ok(())
        }
        Command::ToggleSidetone => toggle_sidetone(cli.mode),
        Command::Tui => run_tui(cli.mode),
    }
}

/// One `--waybar-status` run: always prints a JSON object Waybar can consume.
fn print_waybar_status(mode: Mode) {
    let headset = HeadsetControl::new(mode);

    let output = match headset.status() {
        Ok(status) => waybar::status_output(&status),
        Err(_) => waybar::error_output(mode),
    };

    println!("{}", output.to_json());
}

fn toggle_sidetone(mode: Mode) -> Result<(), Box<dyn std::error::Error>> {
    let config = config::load(mode);
    let level_path = config::level_path(mode);

    // The configured default wins, but when it is unset (0) fall back to the
    // level this app set last, so toggling keeps working after persistence.
    let level = if config.default_sidetone > 0 {
        config.default_sidetone
    } else {
        app::saved_level(&level_path).unwrap_or(0)
    };

    if level == 0 {
        return Ok(());
    }

    let headset = HeadsetControl::new(mode);
    let on = app::toggle_sidetone(&headset, level, &config::state_path(mode))?;

    if on {
        if let Err(err) = app::save_level(&level_path, level) {
            eprintln!("headset-tui: could not save sidetone level: {err}");
        }
    }

    Ok(())
}

fn run_tui(mode: Mode) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = config::load(mode);
    let headset = HeadsetControl::new(mode);
    let level_path = config::level_path(mode);
    let state_path = config::state_path(mode);

    let status = headset.status();
    let reported = status
        .as_ref()
        .ok()
        .and_then(|status| status.sidetone_level);
    let saved = app::saved_level(&level_path);
    let initial = app::initial_level(reported, saved, config.default_sidetone);

    let mut state = UiState::new(initial, mode);
    app::apply_status(&mut state, status);

    match reported {
        // The device can report its level, so trust it and keep the local
        // memory in sync with what is actually configured.
        Some(level) => {
            if let Err(err) = save_sidetone_state(&level_path, &state_path, level) {
                state.last_error = Some(format!("could not save sidetone state: {err}"));
            }
        }
        // The device cannot report its level. Re-apply the remembered level so
        // the sidetone survives a headset power cycle or a new TUI session.
        None if saved.is_some() || config.default_sidetone > 0 => {
            match headset.set_sidetone(initial) {
                Ok(()) => {
                    if let Err(err) = save_sidetone_state(&level_path, &state_path, initial) {
                        state.last_error = Some(format!("could not save sidetone state: {err}"));
                    }
                }
                Err(err) => state.last_error = Some(err.to_string()),
            }
        }
        None => {}
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(
        &mut terminal,
        &mut config.keys,
        &mut state,
        &headset,
        &level_path,
        &state_path,
    );

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    keys: &mut KeyConfig,
    state: &mut UiState,
    headset: &dyn Headset,
    level_path: &Path,
    state_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_refresh = Instant::now();
    let mut editor: Option<ConfigEditor> = None;

    loop {
        terminal.draw(|frame| {
            ui::draw_with_modal(frame, state, keys, editor.as_ref());
        })?;

        let key = input::poll_key();

        // While the modal is open it owns the keyboard.
        if editor.is_some() {
            let outcome = match (editor.as_mut(), key) {
                (Some(active), Some(key)) => active.handle_key(key),
                _ => EditorOutcome::Continue,
            };

            match outcome {
                EditorOutcome::Continue => {}
                EditorOutcome::Cancel => {
                    editor = None;
                    last_refresh = Instant::now();
                }
                EditorOutcome::Save(config) => {
                    let path = config::config_path(state.mode);

                    match config::save_to_path(&path, &config) {
                        Ok(()) => {
                            let default_changed = editor
                                .as_ref()
                                .map(|active| {
                                    active.original.default_sidetone != config.default_sidetone
                                })
                                .unwrap_or(false);

                            *keys = config.keys.clone();
                            editor = None;

                            // A changed default is an explicit choice, so make
                            // it visible immediately instead of only on the
                            // next start. This includes 0 (sidetone off).
                            if default_changed {
                                app::set_sidetone_level(state, headset, config.default_sidetone);

                                if let Err(err) =
                                    save_sidetone_state(level_path, state_path, state.sidetone)
                                {
                                    state.last_error =
                                        Some(format!("could not save sidetone state: {err}"));
                                }
                            }

                            last_refresh = Instant::now();
                        }
                        Err(err) => {
                            if let Some(active) = editor.as_mut() {
                                active.message = Some(format!("Could not save: {err}"));
                            }
                        }
                    }
                }
            }

            continue;
        }

        let action = key
            .map(|key| input::key_to_action(key, keys))
            .unwrap_or(Action::None);

        // Idle ticks refresh the battery display. Polling that often would run
        // headsetcontrol several times per second, so throttle it.
        if action == Action::None && last_refresh.elapsed() >= STATUS_REFRESH {
            let before = state.sidetone;
            app::apply_status(state, headset.status());

            // A device that can report its level may have been changed
            // elsewhere; remember the new value.
            if state.sidetone != before {
                if let Err(err) = save_sidetone_state(level_path, state_path, state.sidetone) {
                    state.last_error = Some(format!("could not save sidetone state: {err}"));
                }
            }

            last_refresh = Instant::now();
        }

        let before = state.sidetone;

        match app::apply_action(action, state, headset) {
            Outcome::Quit => break,
            Outcome::Continue => {}
            Outcome::OpenConfig => {
                editor = Some(ConfigEditor::new(config::load(state.mode)));
            }
        }

        if state.sidetone != before {
            if let Err(err) = save_sidetone_state(level_path, state_path, state.sidetone) {
                state.last_error = Some(format!("could not save sidetone state: {err}"));
            }
        }
    }

    Ok(())
}

/// Remember the level shown in the TUI so the next session (and the Waybar
/// toggle) can restore it on headsets that cannot report their sidetone.
fn save_sidetone_state(level_path: &Path, state_path: &Path, level: u8) -> io::Result<()> {
    app::save_level(level_path, level)?;
    app::save_toggle_state(state_path, level > 0)
}
