# nebucat

nebucat is a keyboard launcher for Windows, written in Rust on GPUI (through gpui-kit). Press `Alt+Space`, type, press `Enter`.

It does six things from one input:

- Finds installed apps (Start menu desktop apps and Store apps) by fuzzy match, ranked higher the more often you launch them.
- Runs a shell command and shows its output inside the launcher, with ANSI colors and pretty-printed JSON.
- Searches files through [Everything](https://www.voidtools.com/).
- Evaluates arithmetic inline and copies the result.
- Optionally offers a web search when nothing else matches.
- Runs plugins: external programs that answer queries over newline-delimited JSON on stdio.

Settings, themes and plugins live in `%APPDATA%\nebucat` and reload when you save them.

## Requirements

- Windows 11, 64-bit.
- Optional: Everything, for `/` file search. Keep it running while you search. nebucat also needs `Everything64.dll` next to `nebucat.exe`. The release zip and the installer include it.
- Optional: Windows Terminal (`wt.exe`), for `Ctrl+Enter` on commands. Without it, nebucat opens `cmd.exe` instead.
- Optional: Python on `PATH`, for the example plugin.

## Install

### Installer

Run in PowerShell:

```powershell
irm https://raw.githubusercontent.com/raditsoic/nebucat/main/install.ps1 | iex
```


The installer:

- Downloads the latest release zip and checks it against `SHA256SUMS`.
- Installs to `%LOCALAPPDATA%\Programs\nebucat`.
- Creates a Start Menu shortcut.

To also install Everything through winget, save the script and run it with `-WithEverything`:

```powershell
irm https://raw.githubusercontent.com/raditsoic/nebucat/main/install.ps1 -OutFile install.ps1
.\install.ps1 -WithEverything
```

To remove nebucat, run `uninstall.ps1`. Add `-Purge` to also delete `%APPDATA%\nebucat` (config, themes, plugins, launch history).

### Release zip

1. Download `nebucat-vX.Y.Z-windows-x64.zip` from the GitHub Releases page.
2. Extract it anywhere. It contains `nebucat.exe`, `Everything64.dll`, `LICENSE`, `THIRD_PARTY.md`, the font license texts (OFL), `README.md` and `examples/plugins/hello`.
3. Run `nebucat.exe`.

### Everything (optional)

`/` file search needs the Everything app running. Install it with winget:

```powershell
winget install voidtools.Everything
```

Everything is separate software with its own license. nebucat does not bundle it. It only loads `Everything64.dll` from the Everything SDK at runtime.

### SmartScreen

`nebucat.exe` is not code-signed, so Windows SmartScreen warns the first time you run it. Choose `More info`, then `Run anyway`.

## Build from source

Native Windows only. GPUI cannot be cross-compiled from another OS.

You need:

- Rust, installed with [rustup](https://rustup.rs/), using the `x86_64-pc-windows-msvc` toolchain.
- Visual Studio Build Tools (or Visual Studio) with the MSVC C++ tools and a Windows SDK. GPUI's release build runs `fxc.exe` from the Windows SDK.

Build and test:

```powershell
cargo build --release -p nebucat
cargo test --workspace
```

The first build compiles GPUI and takes several minutes. The binary is `target\release\nebucat.exe`.

`/` file search needs `Everything64.dll` next to the exe. Download the [Everything SDK zip](https://www.voidtools.com/Everything-SDK.zip), take `dll\Everything64.dll`, and copy it next to `nebucat.exe`. Without the DLL everything else works and `/` searches show a message that the DLL is missing.

Debug builds load shaders from a path baked in at compile time, so run a debug build on the machine that built it.

## Usage

Press the hotkey (default `Alt+Space`) to show the launcher on the monitor under the mouse cursor. Press it again, press `Esc`, or click another window to hide it. Hiding clears the query.

What you type decides the mode:

| Input | Mode | Example |
|---|---|---|
| Plain text | Apps. Fuzzy match on app names. | `vsc` |
| Empty | Recently launched apps. | |
| `>` then a command | Command. Runs it in the configured shell and shows the output. | `>dir C:\` |
| `/` then a search | Files, through Everything. Everything search syntax works. | `/ext:pdf invoice` |
| `!` then an option | Power options: Restart, Shut down, Sleep, Hibernate. Empty `!` lists all four. | `!sleep` |
| Plugin keyword, a space, then text | Plugin. Sends the text to that plugin. | `hi cat` |
| Arithmetic | Calculator row above app results. `Enter` copies the result. | `7/2*3` gives `= 10.5` |

Each row shows its source on the right (`App`, `Command`, `File`, `Calc`, `Web`, `System`, or the plugin name) and, for the first nine actionable rows, a `Ctrl 1` to `Ctrl 9` hint.

The calculator accepts `+ - * / % ^`, parentheses, unary minus and decimals with `.`. `^` binds tightest and is right-associative, so `2^3^2` is `512`. Input needs at least one digit and one binary operator; a bare number stays an app search.

The web row appears only when `[web] enabled = true` and neither an app nor the calculator matched. Otherwise you see `No apps match "<q>". Start with > to run it as a command.`

### Key bindings

| Keys | Action |
|---|---|
| `Alt+Space` | Show or hide the launcher. Configurable with `hotkey`. |
| `Up` / `Down` | Previous or next selectable row. Wraps around. |
| `Shift+Tab` / `Tab` | Same as `Up` / `Down`. |
| `Enter` | Activate the selected row (see below). |
| `Ctrl+Enter` | Secondary action. Command row or output panel: open the command in a new Windows Terminal tab (`cmd /k`). File row: open Explorer with the file selected. Other rows: same as `Enter`. |
| `Ctrl+Shift+Enter` | App row: run as administrator (UAC prompt). |
| `Ctrl+1` ... `Ctrl+9` | Activate the Nth actionable row. Ignored while command output is shown. |
| `Esc` | Command running: cancel it. Output shown: close it and keep the query. Power confirmation shown: back to the list. Otherwise: hide the launcher. |
| `Ctrl+Shift+C` | Copy the selected row's value: app name, command text, file path, URL, or calculator result. With output shown: copy the whole output. |
| `Ctrl+R` | Rebuild the app index. |
| Mouse click on a row | Activate that row. |

What `Enter` does per source:

| Source | Action |
|---|---|
| App | Launch it, record it in history, hide. On failure the launcher stays open with `Couldn't start <name> (error <code>).` |
| Command | Run it and show the output panel. |
| File | Open it with its default app, hide. |
| Calc | Copy the result to the clipboard, hide. |
| Web | Open the search URL in the default browser, hide. |
| System | Ask `<Title> now?`. A second `Enter` hides the launcher and runs it. On failure the launcher reopens with `Couldn't <verb> (error <code>).` |
| Plugin | The item's action: open a URL or path, copy text, run a command, or call back into the plugin. |

### Commands

`>` commands run in `cmd.exe` (or PowerShell, see `[command] shell`), never elevated, with no console window, no stdin, and `%USERPROFILE%` as the working directory. `cmd.exe` runs with code page 65001 and PowerShell with UTF-8 output encoding, so non-ASCII output displays correctly.

- The header shows `Running… <n> s · Esc to cancel`, then `exit <code> · <secs> s` when it ends. A non-zero exit code is shown in the command accent color.
- After `timeout_secs` (default 15) nebucat kills the process tree and shows `Stopped after <n> s.` Commands that wait for input never finish here; use `Ctrl+Enter` to run them in Windows Terminal.
- `Esc` while running kills the process tree and shows `Cancelled.`
- Output is capped at 5000 lines or 2 MiB, then `Output truncated after 5000 lines.` is appended.
- If stdout is a JSON object or array, it is pretty-printed after the command exits.
- Editing the query closes the output panel.

### Files

`/` searches go to Everything after a 120 ms pause in typing. Rows show the file name and its parent folder. If Everything is not running, the launcher tells you to start it (or install it with `winget install voidtools.Everything`). If `Everything64.dll` is not next to `nebucat.exe`, the launcher says the DLL is missing.

### Power options

`!` lists Restart, Shut down, Sleep and Hibernate. Each needs a second `Enter`; `Esc` or typing cancels. Aliases match too: `reboot`, `shutdown`, `power off`, `turn off`, `suspend`, `standby`.

- Restart and Shut down never force apps closed, so an app with unsaved work can stop them, like the Start menu. Shut down uses Fast Startup when it is on, and a full shutdown otherwise.
- Sleep on a Modern Standby PC (`powercfg /a` lists `Standby (S0 Low Power Idle)`) turns the display off, which is how the Start menu enters standby there.
- When hibernation is off, the Hibernate row is replaced by `Hibernate is off. Turn it on with powercfg /hibernate on in an admin terminal.` The list rechecks on every keystroke, so the change shows without a restart.
- `[system] enabled = false` turns `!` off; it then searches apps like any other text.

### App index

The app list comes from the Windows Apps folder (`shell:AppsFolder`). nebucat rebuilds it at startup, on `Ctrl+R`, from the tray menu, and when the launcher opens and the index is older than 5 minutes. The old index stays usable while a rebuild runs.

### Tray and single instance

The tray icon shows the launcher on left click. Its menu has `Show nebucat`, `Open config folder`, `Rebuild app index` and `Quit`.

Starting `nebucat.exe` while it already runs shows the running instance and exits.

## Configuration

nebucat reads `%APPDATA%\nebucat\config.toml`. On first start it writes this file with every key at its default value, and creates empty `themes\` and `plugins\` folders.

Saved changes apply without a restart. If the file fails to parse, nebucat keeps the previous settings and shows a banner: `config.toml: <error>. Using previous settings.` At startup a parse failure falls back to defaults with `config.toml: <error>. Using defaults.` Unknown keys are errors.

| Key | Default | Accepts |
|---|---|---|
| `hotkey` | `"Alt+Space"` | Modifiers `Ctrl`, `Alt`, `Shift`, `Win` plus one key: `A`-`Z`, `0`-`9`, `Space`, `F1`-`F12`. Joined with `+`, case-insensitive. A key alone (`F9`) is allowed but is then unusable in other apps. A single modifier alone (`Win`) fires when tapped without pressing another key, via a low-level keyboard hook. |
| `theme` | `"system"` | `"system"` follows the Windows app theme. `"nebula-dark"` or `"nebula-light"` for a built-in. Any other value loads `themes\<value>.toml`. |
| `autostart` | `true` | `true` adds nebucat to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (value `nebucat`); `false` removes it. Release builds only; debug builds skip it. |
| `max_results` | `8` | App result rows, `1` to `9`. Values outside are clamped. |
| `[command] shell` | `"cmd"` | `"cmd"` or `"powershell"`. |
| `[command] timeout_secs` | `15` | Seconds before a running command is stopped. |
| `[web] enabled` | `false` | `true` adds a web search row when no app or calculator result matches. |
| `[web] url` | `"https://www.google.com/search?q={query}"` | Search URL. `{query}` is replaced with the URL-encoded query (spaces become `+`). |
| `[files] max_results` | `8` | File rows shown for `/` searches. |
| `[system] enabled` | `true` | `true` lets `!` open power options. `false` makes `!` plain app search. |

If the hotkey string is invalid, a `Hotkey` banner shows the parse error. If another app already holds the hotkey, the banner reads `<hotkey> is taken by another app. Change hotkey in config.toml.`

The default file, without its comments:

```toml
hotkey = "Alt+Space"
theme = "system"
autostart = true
max_results = 8

[command]
shell = "cmd"
timeout_secs = 15

[web]
enabled = false
url = "https://www.google.com/search?q={query}"

[files]
max_results = 8

[system]
enabled = true
```

## Themes

Two themes are built in: `nebula-dark` and `nebula-light`. With `theme = "system"` nebucat picks one from the Windows app theme and switches when Windows switches.

A custom theme is a file `%APPDATA%\nebucat\themes\<name>.toml`, selected with `theme = "<name>"`. It starts from a built-in theme and overrides any subset of its colors:

```toml
name = "my-theme"           # optional
base = "nebula-dark"        # optional: "nebula-dark" (default) or "nebula-light"

[colors]                    # any subset of the 8 tokens
background = "#15141B"
accent_app = "#B79CFF"

[ansi]                      # any subset of the 16 ANSI names
red = "#FF8A8A"
```

- Colors are `#RRGGBB` only.
- `[colors]` tokens: `background`, `surface`, `border`, `text`, `muted`, `accent_app`, `accent_command`, `accent_extension`. Their roles are listed in [DESIGN.md](DESIGN.md).
- `[ansi]` names: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, and `bright_` versions of each. They color command output only.
- Errors read `unknown base "x"`, `colors.text: expected #RRGGBB`, or `unknown key "foo"`.

If the theme file is missing or invalid, nebucat uses the built-in theme that matches the Windows app theme and shows the banner `Theme "<name>": <error>`. Saving the active theme file applies it at once.

## Plugins

A plugin is a folder `%APPDATA%\nebucat\plugins\<name>\` with a `plugin.toml` manifest:

```toml
id = "hello"
name = "Hello"                     # shown as the row label
keyword = "hi"                     # type "hi " to query it
command = ["python", "main.py"]    # started in the plugin folder
```

nebucat starts the plugin on the first query for its keyword and keeps it running. Adding, changing or removing a plugin folder takes effect without a restart.

To try the example, copy `examples/plugins/hello` to `%APPDATA%\nebucat\plugins\hello` and type `hi cat`.

The manifest fields, wire protocol, action types, timeouts and error behavior are in [docs/PLUGINS.md](docs/PLUGINS.md).

## File locations

| File | Path |
|---|---|
| Config | `%APPDATA%\nebucat\config.toml` |
| Custom themes | `%APPDATA%\nebucat\themes\<name>.toml` |
| Plugins | `%APPDATA%\nebucat\plugins\<name>\plugin.toml` |
| Launch history | `%APPDATA%\nebucat\history.json` |
| Log | `%LOCALAPPDATA%\nebucat\logs\nebucat.log` (cleared at each start) |
| Installed files (installer) | `%LOCALAPPDATA%\Programs\nebucat` |
| Everything DLL | `Everything64.dll` next to `nebucat.exe` |

## Further reading

- [DESIGN.md](DESIGN.md): palette, typography, layout and motion rules.
- [AGENTS.md](AGENTS.md): guide for coding agents working on this repo.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): crates, modules, threads and channels.
- [docs/PLUGINS.md](docs/PLUGINS.md): plugin protocol.

## Licenses

nebucat is released under the [MIT License](LICENSE). The copyright notice is in that file.

Third-party components, including the Atkinson Hyperlegible fonts (SIL Open Font License 1.1; texts in `assets/fonts/OFL.txt` and `assets/fonts/OFL-Mono.txt`), the Departure Mono font (SIL Open Font License 1.1; text in `assets/fonts/OFL-DepartureMono.txt`) and the Everything SDK DLL (MIT), are listed in [THIRD_PARTY.md](THIRD_PARTY.md).

Everything, the search application, is separate software with its own license. nebucat does not bundle it.

The logo is a pixel-art cat planet. `tools/make_icon.py` holds its pixel grid and generates `assets/logo/nebucat.svg` and the icons in `assets/icon/`.

## Measured

Measured on one machine on 2026-11-18 (Windows 11, Intel Arc Graphics), release build. Your numbers will differ.

| Metric | Value |
| --- | --- |
| Hotkey to first frame (`show latency` in the log), 10 shows | median 13 ms, range 11 to 17 ms |
| App index, 241 apps | 4.0 s (debug builds: 2.0 to 5.5 s) |
| Memory after 60 s hidden | 122.6 MB working set, 78.8 MB private |
| `nebucat.exe` size | 27.7 MB |
| First release build from clean | 5 min 25 s |
