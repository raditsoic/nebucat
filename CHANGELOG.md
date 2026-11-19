# Changelog

## Unreleased

- New logo: a pixel-art cat planet with a pink ring, replacing the placeholder "n" icon.
- Pixel accents: square window corners and rows, Departure Mono for the 11px labels, hints and footers, and the pixel logo in the empty state.

## 0.1.0

First release.

- App search over the Windows Apps folder, with launch history ranking.
- `>` runs a shell command and shows its output inside the launcher, with timeout, cancel and JSON formatting.
- `/` searches files through Everything (optional, separate install).
- Inline calculator and an optional web search fallback.
- Plugins over newline-delimited JSON on stdio.
- Themes and `config.toml`, applied live. Dark and light themes follow Windows.

Known limits: the executable is unsigned, so Windows SmartScreen warns on first run.
