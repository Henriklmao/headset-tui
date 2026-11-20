# headset-tui

[HeadsetControl](https://github.com/Sapd/HeadsetControl) integration for [Waybar](https://github.com/Alexays/Waybar).

A simple Rust and [ratatui](https://ratatui.rs/) based integration of HeadsetControl features into Waybar. Shows wireless headphone battery status with colored icons and provides an interactive TUI for sidetone control.

![Screenshot of TUI interface](docs/TUI.png "Interactive TUI for sidetone control")

> Only works with headphones supported by HeadsetControl. See: [the official repo](https://github.com/Sapd/HeadsetControl/tree/master?tab=readme-ov-file#supported-devices)

## Features

- **Battery Display** - Shows battery percentage with color-coded icons (green >50%, yellow 15-50%, red <15%)
- **Sidetone Control** - Interactive TUI to adjust sidetone levels (0-128)
- **Persistent Sidetone** - The last level set through the app is remembered and restored on the next start
- **Configurable Keys** - Customize all keybindings and default sidetone via interactive config or terminal
- **Test Mode** - `--test` runs the whole tool against HeadsetControl's simulated device, no headset required
- Automatically hidden from your waybar when no headphones are connected. Tui can still be launched from your terminal.

## Sidetone persistence

Many headsets (including several Corsair models) can only *write* the sidetone
level: HeadsetControl reports `This headset doesn't support sidetone-status`
when the level is read. The app therefore never depends on reading the level
back:

- If the headset can report its sidetone (HeadsetControl exposes a `sidetone`
  object in `-o json`), that level is used and kept in sync.
- Otherwise the app remembers the last level it set in
  `~/.cache/headset-tui-sidetone-level` and applies it again when the TUI
  starts, so the sidetone survives restarts and headset power cycles.
- When neither is available, `default_sidetone` from the config is used.

The Waybar toggle (`--toggle-sidetone`) uses `default_sidetone` when it is
non-zero and falls back to the remembered level otherwise.

Changing sidetone outside the app on a headset that cannot report it cannot be
detected; setting it once through the app (or the Waybar toggle) makes that
level the remembered one.

## Installation

Make sure that you have a [Nerdfont](https://www.nerdfonts.com/) installed and configured in Waybar to see the icons.

### [AUR](https://aur.archlinux.org/packages/headset-tui-git)

```bash
yay -S headset-tui-git
# or
paru -S headset-tui-git
```

After installation, the post_install hook will display instructions for Waybar configuration.

### Manual Build

1. Clone the repository:

```bash
git clone https://github.com/Henriklmao/headset-tui.git
cd headset-tui
```

2. Build with Cargo:

```bash
cargo build --release
```

3. Install the binary:

```bash
sudo install -Dm 755 target/release/headset-tui /usr/bin/headset-tui
```

## Configuration

### Waybar

You can modify the position in your waybar manually here `~/.config/waybar/config.jsonc`:

> The following can be automatically added to your waybar config by running `./install-waybar-config.sh`

```jsonc
{
  "modules-right": ["custom/headsetcontrol"], # You can use modules-left or modules-center instead if you prefer.

  "custom/headsetcontrol": {
      "exec": "/home/$USER/.local/bin/headset-tui/headset-tui --waybar-status", #
      "return-type": "json",
      "interval": 10,
      "format": "{text}",
      "tooltip": true,
      "on-click": "kitty /home/$USER/.local/bin/headset-tui/headset-tui", # You can set any terminal of your choice, however only kitty and alacritty are tested.
      "on-right-click": "bash -c '/home/$USER/.local/bin/headset-tui/headset-tui --toggle-sidetone &'"
    },
}
```

### Keybindings & Settings

Configuration is automatically created at `~/.config/headset-tui/config.toml`.

To configure interactively, either:

- Run: `headset-tui --config`
- Or press `c` inside the tui to interactively set your bindings.

Default keybindings:

- **a/s** - Decrease sidetone by 1/10
- **d/w** - Increase sidetone by 1/10
- **f** - Set sidetone to full (128)
- **e** - Set sidetone to none (0)
- **c** - Open configuration menu
- **q/ESC** - Quit

## Usage

Launch the interactive TUI:

> Same as left click on the waybar symbol, but you can also run it directly in the terminal.

```bash
headset-tui
```

Get battery status for Waybar:

> This prints the exact output thats routed into your waybar

```bash
headset-tui --waybar-status
```

Show all options:

```bash
headset-tui --help
```

## Testing without a headset

`--test` runs the whole tool simulated, so any feature can be tried without
hardware and without touching your real setup. It works with every command and
in any position:

```bash
headset-tui --test                    # TUI with the simulated device
headset-tui --test --waybar-status    # Waybar JSON from the simulated device
headset-tui --toggle-sidetone --test  # Toggle sidetone on the simulated device
headset-tui --config --test           # Edit the test configuration
```

What `--test` changes:

| | Normal | `--test` |
|---|---|---|
| Headset | real device | HeadsetControl mock (`--test-device`) |
| Config | `~/.config/headset-tui/config.toml` | `~/.config/headset-tui/config.test.toml` |
| Sidetone toggle state | `~/.cache/headset-tui-sidetone-state` | `~/.cache/headset-tui-sidetone-state.test` |
| Last set sidetone | `~/.cache/headset-tui-sidetone-level` | `~/.cache/headset-tui-sidetone-level.test` |

The mock device reports a battery level of 42%, advertises sidetone support
(including a readable sidetone level of 85) and does not keep modifications
between invocations, so the TUI starts from the level reported by the mock.

Notes:

- `--test` requires a HeadsetControl build that ships `--test-device`
  (`headsetcontrol --help-all` lists it under `ADVANCED`).
- The interactive config dialog (`--config --test` and the `c` key inside the
  TUI) edits the test config, so experiments never leak into your real
  keybindings or default sidetone.
- Unknown arguments and conflicting commands are rejected with exit code 2
  instead of silently starting the TUI, which keeps typos in a Waybar `exec`
  visible.

## Development

The code is split into small modules so everything can be exercised without a
headset:

| Module | Responsibility |
|---|---|
| `src/cli.rs` | argument parsing, `Mode` (normal/test), help text |
| `src/headset.rs` | HeadsetControl protocol, `Headset` trait and the real backend |
| `src/waybar.rs` | Waybar JSON output |
| `src/app.rs` | UI state, key actions, sidetone toggle persistence |
| `src/config.rs` | config/state paths, reading/writing TOML, config dialog |
| `src/input.rs` | key events to actions |
| `src/ui.rs` | ratatui rendering |
| `src/main.rs` | wiring and the terminal event loop |

Run the test suite and lints:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Device access sits behind the `Headset` trait, so tests use
`headset::mock::MockHeadset` instead of invoking `headsetcontrol`.
