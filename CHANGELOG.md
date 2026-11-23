# Changelog

## 0.3.0

- `!` lists power options: Restart, Shut down, Sleep and Hibernate, each with a pixel glyph. Each needs a second Enter. Shut down behaves like the Start menu, with Fast Startup when it is on. Sleep on Modern Standby PCs turns the display off like the Start menu does.
- `[system] enabled = false` turns the `!` prefix off.
- Plugin keywords may no longer start with `!`.

## 0.2.0

- `hotkey` accepts a single key without a modifier (`F9`) and a single modifier tapped alone (`Win`), using a low-level keyboard hook for the latter.
- Icons that leave the result list are freed from the GPU sprite atlas. Before, every file search and app reindex left its icons behind, so memory grew over the session.
- `/` file search shares one icon per file type instead of extracting one per result, and reads size and date from Everything instead of touching each file on disk.
- The periodic app reindex keeps existing icons. Ctrl+R and the tray's reload still refresh them.

## 0.1.0

First release.

- App search over the Windows Apps folder, with launch history ranking.
- `>` runs a shell command and shows its output inside the launcher, with timeout, cancel and JSON formatting.
- `/` searches files through Everything (optional, separate install).
- Inline calculator and an optional web search fallback.
- Plugins over newline-delimited JSON on stdio.
- Themes and `config.toml`, applied live. Dark and light themes follow Windows.
- Pixel-art cat planet logo, square window corners and rows, and Departure Mono for the 11px labels, hints and footers.

Known limits: the executable is unsigned, so Windows SmartScreen warns on first run.
