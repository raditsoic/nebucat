# nebucat design

Read this file before you change any UI. Every decision below carries its reason. If a change conflicts with a rule here, update this file in the same change and say why.

## Design Read

Keyboard launcher for one developer on Windows 11, own identity (not native Fluent), dial ENERGY 2 / RHYTHM 1 / MOTION 2.

## Palette

Two themes ship: `nebula-dark` and `nebula-light`. The source of truth for the values is `themes/nebula-dark.toml` and `themes/nebula-light.toml`; keep them in sync with this table.

| Token | nebula-dark | nebula-light | Role |
|---|---|---|---|
| background | `#15141B` | `#F0EFF4` | window |
| surface | `#262431` | `#DEDCE6` | selected row; hover uses it at 50% alpha |
| border | `#2C2A36` | `#CFCCD9` | divider under the query, DWM window border (non-text) |
| text | `#ECEAF2` | `#15141B` | primary text |
| muted | `#A09DB0` | `#555168` | subtitles, hints, labels |
| accent_app | `#B79CFF` | `#6D3FD1` | apps, calculator, web fallback, caret, focus ring |
| accent_command | `#FF7AC6` | `#B8146F` | `>` prompt glyph in command output, non-zero exit code |
| accent_extension | `#E08CFF` | `#8E2BB0` | files and plugin rows |

### Contrast

Every text pair below passes WCAG AA (4.5:1 for normal text). `border` is non-text and is not checked against text thresholds.

| Token | dark on background | dark on surface | light on background | light on surface |
|---|---|---|---|---|
| text | 15.35 | 12.77 | 15.99 | 13.49 |
| muted | 6.92 | 5.76 | 6.64 | 5.6 |
| accent_app | 8.03 | 6.68 | 5.59 | 4.72 |
| accent_command | 7.71 | 6.42 | 5.45 | 4.6 |
| accent_extension | 8.16 | 6.79 | 5.88 | 4.96 |

Reason for the surface column: selected rows draw text on `surface`, so every text color must also pass there.

## ANSI palettes

These colors are content colors for command output only. UI chrome never uses them. Every entry is at least 4.5:1 on its theme's background.

| Name | dark | ratio on `#15141B` | light | ratio on `#F0EFF4` |
|---|---|---|---|---|
| black | `#8C899C` | 5.38 | `#15141B` | 15.99 |
| red | `#FF8A8A` | 8.06 | `#B42318` | 5.75 |
| green | `#8FD98F` | 10.88 | `#2F6F2F` | 5.34 |
| yellow | `#E8C66A` | 11.08 | `#7A5A00` | 5.58 |
| blue | `#8AB4FF` | 8.76 | `#1F5BBF` | 5.55 |
| magenta | `#FF7AC6` | 7.71 | `#B8146F` | 5.45 |
| cyan | `#6FD3D3` | 10.4 | `#1F6E6E` | 5.22 |
| white | `#D6D3E0` | 12.42 | `#555168` | 6.64 |
| bright_black | `#A09DB0` | 6.92 | `#555168` | 6.64 |
| bright_red | `#FFA8A8` | 9.95 | `#C4321F` | 4.8 |
| bright_green | `#A9EBA9` | 13.19 | `#357A35` | 4.6 |
| bright_yellow | `#F5DA8C` | 13.32 | `#86640A` | 4.78 |
| bright_blue | `#A9C8FF` | 10.8 | `#2B67CC` | 4.69 |
| bright_magenta | `#FF9AD5` | 9.44 | `#A3135F` | 6.52 |
| bright_cyan | `#93E3E3` | 12.49 | `#236F75` | 5.1 |
| bright_white | `#ECEAF2` | 15.35 | `#3A3747` | 10.1 |

Reason: programs print with standard ANSI colors, and the stock terminal values fail contrast on these backgrounds. Black and white are remapped so they stay readable on both themes.

## Owner overrides

- **R-01:** a purple accent on a near-black base is kept. Reason: the brand name nebucat comes from nebula. The accent stays flat: no gradients, glow or starfield.
- **R-29:** three accent tones are kept. Reason: each marks one result source, and a text label carries the same information, so color is never the only cue.

## Accents color text only

Accents color matched letters, the prompt glyph, source labels and the exit code. They never color backgrounds, borders or glow. Reason: the selected row and the window already separate by `surface` and `border`, so accent fills would only add noise.

## Typography

- **UI font:** Atkinson Hyperlegible Next. Reason: its letterforms keep near-identical app names distinguishable when scanning fast.
- **Mono font:** Atkinson Hyperlegible Mono, for command output, source labels and shortcut hints. Reason: same letterform family, so output and UI read as one voice.
- **Query size:** 16px, not 22px. Reason: the query is typed text and should not dominate the window. It stays in Next because Mono letterforms are wide and space the typed text too far apart.
- Fonts are bundled from `assets/fonts/` under the SIL Open Font License (`assets/fonts/OFL.txt`). Reason: the launcher must look the same on a machine without them installed.

| Element | Font | Size | Weight |
|---|---|---|---|
| Query | Next | 16px | Regular |
| Row title | Next | 14px | Regular |
| Matched letters in a title | Next | 14px | SemiBold 600 |
| Subtitle | Next | 12px | Regular |
| Source labels and shortcut hints | Mono | 11px | Regular |
| Command output | Mono | 13px, 20px line height | Regular |

Reason for SemiBold matches: weight plus accent color marks the match, so it still reads for users who cannot see the hue.

## Shape

- **Window corners:** Windows 11 DWM round (`DWMWCP_ROUND`). Reason: matches every other window on the desktop without drawing custom corners.
- **Row radius:** 2px. No pill shapes. Reason: sharp corners match a terminal's grid and the dev-tool direction; rows are list items, not buttons.
- **Row layout:** one line, 36px tall: icon, title, subtitle, file size and age, source label, shortcut hint. Title and subtitle are two equal columns, and the subtitle is muted and cut with an ellipsis. Reason: more results fit, and the path sits beside the name instead of under it.
- **Spacing scale:** 4, 8, 12, 16, 20, 24. Reason: one scale keeps every gap on the same rhythm (RHYTHM 1).
- **Shadow:** only the DWM window shadow. Reason: the launcher floats above other apps; rows sit flat.

## Theme mode

The default is `system`: nebucat follows the Windows app theme, and switches live when the user changes it. Both themes ship and are verified. Reason: R-21 gives no strong reason for a fixed theme.

## Identity motif

Each row ends in a right-aligned column. It holds the source label (`App`, `File`, plugin name, `Calc`, `Web`) in that source's accent, then the `Ctrl 1` to `Ctrl 9` shortcut hint in muted Mono. Notice rows have no label and no hint. Command mode shows no result row: Enter runs the typed command, so a row would only repeat the query.

| Label | Accent |
|---|---|
| `App`, `Calc`, `Web` | accent_app |
| `File`, plugin name | accent_extension |

Reason: the label tells the user where a result came from in text, and the hint teaches the fastest way to pick it.

File rows show size and age (`12.4K  3d`) in muted Mono 11, in the label column before the source label. Reason: the size and age show at a glance, like `ls -lh`. Age is relative, so it needs no timezone.

### Results footer

Shown under the results when at least one row is selectable, and always in command mode, where there are no rows. Right-aligned, muted Mono 11. Results: `Enter open · Ctrl 1-9 pick · Esc hide`. Command mode copy is in the Command mode section. Reason: a status line keeps the key hints visible without a help screen, and in command mode it replaces the row that only repeated the query.

## Motion (MOTION 2)

- The command output body fades in from opacity 0 to 1 over 120 ms. Reason: it shows that new content arrived without layout jumps.
- No other animation, and no looping motion. Reason: a launcher opens dozens of times a day, and motion would slow every one of them.
- The running state shows an elapsed-seconds counter, not a spinner. Reason: a number tells the user how long the command has run; a spinner only says that it runs.
- The fade is disabled when Windows "Animation effects" is off (`SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)`). Reason: respect the user's reduced-motion setting.

## Icon

A 32×32 pixel-art cat whose round head is a planet with a pink ring, on a `#15141B` square with stepped corners. The cat is `#B79CFF` with a `#8B6BE0` shade; the ring's front is `#FF7AC6` over `#C9509A` and its back is `#9A5BC0`. Every color is flat. `tools/make_icon.py` holds the pixel grid and writes `assets/logo/nebucat.svg` and every size in `assets/icon/`; edit the grid there, never the outputs. Reason: the name joins nebula and cat, and pixel art stays legible down to the 16px tray icon.

## Icons in rows

Rows show only real app and file icons from the Windows shell, in a reserved 24×24 slot. No icon library glyphs. Reason: R-04. gpui-kit bundles Lucide icons; do not use them in result rows. The slot is reserved even before an icon loads, so rows do not shift.

## State copy

These strings are exact. Placeholders in angle brackets are filled at runtime. Change a string here first, then in code.

### Query input

- Placeholder: `Type an app, > for a command, / for files`
- In command mode, Up and Down recall earlier commands like a shell, newest first. Down past the newest restores what was typed. History keeps 200 unique commands in `%LOCALAPPDATA%\nebucat\command_history.txt`. Reason: developers rerun commands.

### Apps

- Empty query, no history: `Type an app name. Start with > to run a command or / to search files.`
- Index not ready: `Indexing apps…`
- Index failed (action: rebuild the index): `Couldn't read the Apps folder: <error>. Press Ctrl+R to retry.`
- No match: `No apps match "<q>". Start with > to run it as a command.` The web row follows it when the web fallback is enabled.
- Failed launch (the window stays open): `Couldn't start <name> (error <code>).`

### Command mode

- Empty command: no row. The footer reads `Type a command · Up recalls history · Esc hide`.
- Typed command: no row. The footer reads `Enter run · Ctrl+Enter in Terminal · Up/Down history · Esc hide`.
- Status while running: `Running… <n> s · Esc to cancel`
- Status after exit 0: `exit 0 · <secs> s`
- Status after a non-zero exit: `exit <code>` in accent_command, then `· <secs> s` in muted.
- Status after timeout: `Stopped after <n> s.`
- Extra muted line after timeout: `Ctrl+Enter opens it in Windows Terminal for interactive commands.`
- Status after Esc: `Cancelled.`
- Status when the shell fails to start: `Couldn't start the shell: <e>`
- Muted line when output hits the cap: `Output truncated after 5000 lines.`
- Footer, one line in two columns. Left: `Enter run · Ctrl+Shift+C copy · Ctrl+Enter in Terminal · Esc clear`, without `Esc clear` while running. Right: the status. Reason: the status sits beside the hints instead of in a header row above the output, so the output starts right under the query. The command is not repeated, because the query bar already holds it.
- The output stays while you edit the command and is replaced only when you run again. Esc clears it. Leaving command mode (deleting `>`) also clears it. Reason: you can tweak a command while reading the last result.

### Files

- Empty query: `Type a file name after /. Everything search syntax works, like ext:pdf.`
- Everything not running: `Everything isn't running. Install it with winget install voidtools.Everything, then start it.`
- DLL missing: `Everything64.dll is missing next to nebucat.exe. File search needs it. See the README.`
- No results: `No files match "<q>".`
- Search takes longer than 300 ms: `Searching…`

### Calculator and web

- Calculator row title: `= <result>`
- Calculator row subtitle: `Enter copies the result`
- Web row title: `Search the web for "<q>"`

### Plugins

- Keyword rejected (banner): `Plugin <dir>: keyword "<k>" not allowed`
- Keyword taken (banner): `Plugin <dir>: keyword "<k>" already used by <other>`
- Plugin returned an error: `<name>: <message>`
- Request timed out: `<name> didn't answer in 2 s.`
- Process exited: `<name> stopped: <last stderr line or "exit <code>">`

### Config, theme and hotkey banners

The banner is a 36px row above the results. A muted Mono 11 label (`Config`, `Hotkey`, `Theme` or `Plugin`) comes first, then the message in `text`. The banner stays until its cause is fixed.

- Config invalid at startup: `config.toml: <error>. Using defaults.`
- Config invalid on reload: `config.toml: <error>. Using previous settings.`
- Theme missing or invalid: `Theme "<value>": <error>`
- Hotkey taken: `<hotkey> is taken by another app. Change hotkey in config.toml.`
