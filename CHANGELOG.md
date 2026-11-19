# Changelog

## 0.1.0

First release.

- App search over the Windows Apps folder, with launch history ranking.
- `>` runs a shell command and shows its output inside the launcher, with timeout, cancel and JSON formatting.
- `/` searches files through Everything (optional, separate install).
- Inline calculator and an optional web search fallback.
- Plugins over newline-delimited JSON on stdio.
- Themes and `config.toml`, applied live. Dark and light themes follow Windows.

Known limits: the executable is unsigned, so Windows SmartScreen warns on first run. The icon is a placeholder.
