# AGENTS.md

Guide for coding agents working on nebucat.

## What nebucat is

nebucat is a keyboard launcher for Windows 11, written in Rust on GPUI (via gpui-kit). Press `Alt+Space`, type, press `Enter`. It searches installed apps, runs shell commands and shows their output inside the launcher, searches files through Everything, evaluates math inline, can fall back to a web search, and loads plugins that speak newline-delimited JSON over stdio. Themes and settings live in `%APPDATA%\nebucat` and reload live.

## Repo layout

```
Cargo.toml                 workspace (resolver 3, edition 2024, dev opt-levels for gpui crates)
crates/
  nebucat-core/            pure Rust lib, no Windows deps, tests run on any OS
    src/
      ansi.rs              ANSI/SGR bytes -> colored spans (vte)
      calc.rs              f64 expression parser and result formatting
      config.rs            config.toml schema, defaults, DEFAULT_FILE
      fuzzy.rs             nucleo-matcher ranking with history boosts
      history.rs           launch counts, recency, boost formula
      hotkey.rs            "Alt+Space" style hotkey parsing
      json.rs              pretty-print and tokenize JSON output
      plugin_proto.rs      plugin manifest and wire types
      power.rs             power actions, aliases, fuzzy matching for the ! list
      query.rs             input -> Mode (apps, >command, /files, !power, plugin keyword)
      theme.rs             Theme struct, built-ins, theme file parsing
      url.rs               query-string encoding for the web fallback
  nebucat/                 Windows binary
    build.rs               embeds the icon resource via winresource
    src/
      main.rs              single-instance mutex/event, logging, gpui start
      paths.rs             config_dir, data_dir, log file
      shell.rs             Win32 thread: hotkey, tray, show event
      window_ctl.rs        show/hide, monitor placement, foreground focus
      app.rs               Launcher view, query routing, keymap, window height
      autostart.rs         HKCU Run key (release builds only)
      apps.rs              AppsFolder index, icons, launch
      command.rs           > command runner, job object, output panel state
      files.rs             Everything SDK thread and file rows
      plugins.rs           plugin discovery, processes, protocol
      power.rs             power capabilities, shutdown privilege, restart/shut down/sleep/hibernate
      ui/theme.rs          applies core Theme to gpui-kit, ActiveTheme global
themes/                    nebula-dark.toml, nebula-light.toml (built-ins, include_str!)
assets/fonts/              Atkinson Hyperlegible Next and Mono, Departure Mono, OFL texts
assets/icon/               generated icon (ico, png, raw rgba)
assets/logo/nebucat.svg    generated pixel-art logo
tools/make_icon.py         holds the logo pixel grid, generates assets/logo and assets/icon (python3, stdlib only)
scripts/release.ps1        builds the release zip and SHA256SUMS
examples/plugins/hello/    example plugin (plugin.toml, main.py)
docs/                      ARCHITECTURE.md, PLUGINS.md
DESIGN.md                  visual direction, palette, type, motion, state copy
install.ps1                downloads the latest release, installs, adds a Start Menu shortcut
uninstall.ps1              removes the install; -Purge also removes %APPDATA%\nebucat
LICENSE                    MIT
THIRD_PARTY.md             third-party licenses and notices
CHANGELOG.md               release notes
```

## Build and test

Core logic builds and tests on any OS:

```sh
cargo test -p nebucat-core
```

The Windows binary builds on Windows only. Install Rust through rustup with the MSVC toolchain, plus Visual Studio Build Tools with the Windows SDK.

```sh
cargo build -p nebucat             # debug build
cargo build --release -p nebucat   # release build
cargo test --workspace             # all crates
```

GPUI cannot be cross-compiled from Linux or macOS. Its release build runs `fxc.exe` from the Windows SDK, and debug builds load shaders from a path baked in at compile time, so run a debug build on the machine that built it.

`Everything64.dll` must sit beside `nebucat.exe`, but only the `/` file search needs it. Download the Everything SDK zip from https://www.voidtools.com/Everything-SDK.zip and copy `dll/Everything64.dll` next to the exe. It is never committed.

The log is `%LOCALAPPDATA%\nebucat\logs\nebucat.log`.

## UI rules

- Read `DESIGN.md` before any UI change. Palette, type sizes, spacing, motion and state copy come from there, not from defaults.
- Apply the antislop skills (`antislop`, `antislop-ui`, `antislop-code`) in "during" mode while writing UI.
- Run the antislop Delivery Gate before handing off any UI work, and report PASS/FAIL per line with evidence.
- No em dashes in UI copy. Use the exact strings from `DESIGN.md` and the plan.
- Result rows use only real shell icons for apps and files. Do not use the bundled Lucide glyphs in rows. Power rows, which have no shell icon, use the pixel glyphs in `crates/nebucat/src/pixel.rs`.

## Comment rules

- One-line comments that state a constraint the code cannot show (an API quirk, a threading rule, a Win32 requirement).
- No banners, no step narration, no vague TODOs.
- No em dashes anywhere in code or docs.

## Threading model

- **gpui main thread.** Owns the `Launcher` view, the window and all UI state. Receives work from other threads over `async_channel` in `cx.spawn` tasks.
- **Shell thread** (`shell.rs`). A plain `std::thread` with its own Win32 message loop, independent of gpui's loop. It owns the global hotkey (`RegisterHotKey`), the tray icon (tray-icon requires its creating thread to pump messages) and the single-instance show event. It sends `ShellEvent`s to gpui. `ShellHandle::set_hotkey` re-registers through `PostThreadMessageW(WM_APP + 1)`.
- **App index thread** (`apps.rs`). Initializes COM as STA, enumerates `FOLDERID_AppsFolder`, extracts icons, sends the app list back. The old index keeps serving while it runs.
- **Everything thread** (`files.rs`). One thread owns `Everything64.dll` because the SDK is not thread-safe. All queries go through it.
- **Command reader threads** (`command.rs`). Two per running command (stdout, stderr) append into a shared buffer that the UI polls every 50 ms. The child runs in a Job Object with kill-on-close, so cancel and timeout kill the whole tree.
- **Plugin child processes** (`plugins.rs`). One long-lived process per plugin, spawned lazily, speaking newline-delimited JSON on stdin/stdout.
- **Power thread** (`power.rs`). One short-lived thread per confirmed power action, because `SetSuspendState` blocks until the PC wakes. It replies once over `async_channel`.

Cross-thread results reach the gpui side through `async_channel`. Never call gpui APIs off the main thread.

## Plugin protocol

`crates/nebucat-core/src/plugin_proto.rs` defines the wire types documented in `docs/PLUGINS.md`. Change both in the same commit and keep them in sync.
