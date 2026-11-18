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

- Windows 11.
- Optional: Everything, for `/` file search. Install it with `winget install voidtools.Everything` and keep it running. nebucat also needs `Everything64.dll` next to `nebucat.exe`; `scripts/win.sh build` puts it there.
- Optional: Windows Terminal (`wt.exe`), for `Ctrl+Enter` on commands. Without it, nebucat opens `cmd.exe` instead.
- Optional: Python on `PATH`, for the example plugin.

## Build

The source lives in WSL. GPUI cannot be cross-compiled from Linux, so the binary is built by the Windows Rust toolchain in a copy of the tree on the Windows drive. `scripts/win.sh` copies the tree and runs `cargo.exe` there.

Windows needs:

- Rust with the `x86_64-pc-windows-msvc` host.
- Visual Studio 2022 or its Build Tools (MSVC), and the Windows SDK (GPUI's build runs `fxc.exe` from it).

WSL needs `bash`, `tar`, `curl` and `unzip`.

`scripts/win.sh` has two machine-specific constants at the top. Change them for another machine:

```bash
WIN=/mnt/c/Users/radit/dev/nebucat              # Windows build copy
CARGO=/mnt/c/Users/radit/.cargo/bin/cargo.exe   # Windows cargo
```

Commands, run from the repo root in WSL:

| Command | What it does |
|---|---|
| `cargo test -p nebucat-core` | Runs the core logic tests natively in WSL. `nebucat-core` has no Windows dependencies. |
| `scripts/win.sh sync` | Copies the tree to the Windows build copy. Changed files only; files deleted in WSL are deleted there too. `target/` and `vendor/` are kept. |
| `scripts/win.sh build [--release]` | `sync`, then `cargo build -p nebucat` on Windows. Copies `Cargo.lock` back to the repo. Downloads the Everything SDK into `vendor/` once and copies `Everything64.dll` next to the exe. |
| `scripts/win.sh run [--release]` | Stops a running nebucat, builds, then starts `target\<profile>\nebucat.exe`. |
| `scripts/win.sh kill` | Stops `nebucat.exe`. |
| `scripts/win.sh test` | `sync`, then `cargo test --workspace` on Windows. |
| `scripts/win.sh log` | Prints the last 200 lines of the log. |

The first build compiles GPUI and takes several minutes. The release binary is `target\release\nebucat.exe` in the Windows build copy.

Do not edit the Windows build copy directly. The next `sync` overwrites it.

`Everything64.dll` is downloaded at build time and is not committed to this repo.

## Usage

Press the hotkey (default `Alt+Space`) to show the launcher on the monitor under the mouse cursor. Press it again, press `Esc`, or click another window to hide it. Hiding clears the query.

What you type decides the mode:

| Input | Mode | Example |
|---|---|---|
| Plain text | Apps. Fuzzy match on app names. | `vsc` |
| Empty | Recently launched apps. | |
| `>` then a command | Command. Runs it in the configured shell and shows the output. | `>dir C:\` |
| `/` then a search | Files, through Everything. Everything search syntax works. | `/ext:pdf invoice` |
| Plugin keyword, a space, then text | Plugin. Sends the text to that plugin. | `hi cat` |
| Arithmetic | Calculator row above app results. `Enter` copies the result. | `7/2*3` gives `= 10.5` |

Each row shows its source on the right (`App`, `Command`, `File`, `Calc`, `Web`, or the plugin name) and, for the first nine actionable rows, a `Ctrl 1` to `Ctrl 9` hint.

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
| `Esc` | Command running: cancel it. Output shown: close it and keep the query. Otherwise: hide the launcher. |
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

`/` searches go to Everything after a 120 ms pause in typing. Rows show the file name and its parent folder. If Everything is not running, the launcher says `Everything isn't running. Install it with winget install voidtools.Everything, then start it.` If the DLL is missing, it says `Everything64.dll is missing next to nebucat.exe. Run scripts/win.sh build.`

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
| `hotkey` | `"Alt+Space"` | One or more of `Ctrl`, `Alt`, `Shift`, `Win`, plus exactly one key: `A`-`Z`, `0`-`9`, `Space`, `F1`-`F12`. Joined with `+`, case-insensitive. At least one modifier is required. |
| `theme` | `"system"` | `"system"` follows the Windows app theme. `"nebula-dark"` or `"nebula-light"` for a built-in. Any other value loads `themes\<value>.toml`. |
| `autostart` | `true` | `true` adds nebucat to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (value `nebucat`); `false` removes it. Release builds only; debug builds skip it. |
| `max_results` | `8` | App result rows, `1` to `9`. Values outside are clamped. |
| `[command] shell` | `"cmd"` | `"cmd"` or `"powershell"`. |
| `[command] timeout_secs` | `15` | Seconds before a running command is stopped. |
| `[web] enabled` | `false` | `true` adds a web search row when no app or calculator result matches. |
| `[web] url` | `"https://www.google.com/search?q={query}"` | Search URL. `{query}` is replaced with the URL-encoded query (spaces become `+`). |
| `[files] max_results` | `8` | File rows shown for `/` searches. |

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
| Everything DLL | `Everything64.dll` next to `nebucat.exe` |

## Further reading

- [DESIGN.md](DESIGN.md): palette, typography, layout and motion rules.
- [AGENTS.md](AGENTS.md): guide for coding agents working on this repo.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): crates, modules, threads and channels.
- [docs/PLUGINS.md](docs/PLUGINS.md): plugin protocol.

## Credits

- UI fonts: Atkinson Hyperlegible Next and Atkinson Hyperlegible Mono, under the SIL Open Font License 1.1. License texts: `assets/fonts/OFL.txt` and `assets/fonts/OFL-Mono.txt`.
- The icon (a lowercase "n" on a dark rounded square) is a placeholder until a real logo exists. `tools/make_icon.py` generates it.

## Measured

Pending: filled in after the verification run.
