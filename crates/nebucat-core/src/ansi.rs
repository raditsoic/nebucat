use vte::{Params, Parser, Perform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    /// Index 0..=15 into the theme's ANSI palette.
    Ansi(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub fg: Color,
    pub bold: bool,
}

pub fn parse_lines(bytes: &[u8]) -> Vec<Vec<Span>> {
    let mut state = State::default();
    let mut parser = Parser::new();
    parser.advance(&mut state, bytes);
    if !state.line.is_empty() {
        state.lines.push(std::mem::take(&mut state.line));
    }
    state.lines
}

struct State {
    lines: Vec<Vec<Span>>,
    line: Vec<Span>,
    col: usize,
    // `\r` clears the line only when text follows, so CRLF keeps its content.
    pending_cr: bool,
    fg: Color,
    bold: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            lines: Vec::new(),
            line: Vec::new(),
            col: 0,
            pending_cr: false,
            fg: Color::Default,
            bold: false,
        }
    }
}

impl State {
    fn push_char(&mut self, c: char) {
        if self.pending_cr {
            self.line.clear();
            self.col = 0;
            self.pending_cr = false;
        }
        match self.line.last_mut() {
            Some(span) if span.fg == self.fg && span.bold == self.bold => span.text.push(c),
            _ => self.line.push(Span {
                text: c.to_string(),
                fg: self.fg,
                bold: self.bold,
            }),
        }
        self.col += 1;
    }

    fn sgr(&mut self, params: &Params) {
        let items: Vec<&[u16]> = params.iter().collect();
        if items.is_empty() {
            self.fg = Color::Default;
            self.bold = false;
            return;
        }
        let mut i = 0;
        while i < items.len() {
            let Some(&code) = items[i].first() else {
                i += 1;
                continue;
            };
            let item = items[i];
            i += 1;
            match code {
                0 => {
                    self.fg = Color::Default;
                    self.bold = false;
                }
                1 => self.bold = true,
                22 => self.bold = false,
                n @ 30..=37 => self.fg = Color::Ansi((n - 30) as u8),
                n @ 90..=97 => self.fg = Color::Ansi((n - 90 + 8) as u8),
                39 => self.fg = Color::Default,
                n @ (38 | 48) => {
                    // Colon form packs the extended color into one item; semicolon form spreads it.
                    let rest: Vec<u16> = if item.len() > 1 {
                        item[1..].to_vec()
                    } else {
                        let mode = items.get(i).and_then(|p| p.first()).copied();
                        let used = match mode {
                            Some(5) => 2,
                            Some(2) => 4,
                            _ => 0,
                        };
                        let end = (i + used).min(items.len());
                        let rest = items[i..end].iter().filter_map(|p| p.first().copied()).collect();
                        i = end;
                        rest
                    };
                    if n == 38
                        && let Some(color) = extended_color(&rest)
                    {
                        self.fg = color;
                    }
                }
                _ => {}
            }
        }
    }
}

fn extended_color(rest: &[u16]) -> Option<Color> {
    match rest {
        [5, n, ..] if *n < 16 => Some(Color::Ansi(*n as u8)),
        [5, n, ..] if *n < 256 => Some(nearest_ansi(xterm_rgb(*n as u8))),
        [2, r, g, b] | [2, _, r, g, b, ..] => Some(Color::Rgb(*r as u8, *g as u8, *b as u8)),
        _ => None,
    }
}

const XTERM_16: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 0, 0),
    (0, 205, 0),
    (205, 205, 0),
    (0, 0, 238),
    (205, 0, 205),
    (0, 205, 205),
    (229, 229, 229),
    (127, 127, 127),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (92, 92, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

fn xterm_rgb(n: u8) -> (u8, u8, u8) {
    match n {
        0..=15 => XTERM_16[n as usize],
        16..=231 => {
            let i = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            (level(i / 36), level((i / 6) % 6), level(i % 6))
        }
        _ => {
            let g = 8 + 10 * (n - 232);
            (g, g, g)
        }
    }
}

fn nearest_ansi((r, g, b): (u8, u8, u8)) -> Color {
    let dist = |&(cr, cg, cb): &(u8, u8, u8)| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(r, cr) + d(g, cg) + d(b, cb)
    };
    let idx = (0..16).min_by_key(|&i| dist(&XTERM_16[i])).unwrap_or(0);
    Color::Ansi(idx as u8)
}

impl Perform for State {
    fn print(&mut self, c: char) {
        self.push_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.lines.push(std::mem::take(&mut self.line));
                self.col = 0;
                self.pending_cr = false;
            }
            b'\r' => self.pending_cr = true,
            b'\t' => {
                let spaces = 4 - self.col % 4;
                for _ in 0..spaces {
                    self.push_char(' ');
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        if action == 'm' {
            self.sgr(params);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, fg: Color, bold: bool) -> Span {
        Span {
            text: text.to_string(),
            fg,
            bold,
        }
    }

    #[test]
    fn red_then_reset() {
        let lines = parse_lines(b"\x1b[31mred\x1b[0m ok\n");
        assert_eq!(
            lines,
            vec![vec![
                span("red", Color::Ansi(1), false),
                span(" ok", Color::Default, false),
            ]]
        );
    }

    #[test]
    fn trailing_line_without_newline() {
        assert_eq!(
            parse_lines(b"a\nb"),
            vec![vec![span("a", Color::Default, false)], vec![span("b", Color::Default, false)]]
        );
        assert!(parse_lines(b"").is_empty());
    }

    #[test]
    fn crlf_keeps_text_and_bare_cr_overwrites() {
        assert_eq!(parse_lines(b"dir\r\n"), vec![vec![span("dir", Color::Default, false)]]);
        assert_eq!(parse_lines(b"10%\r99%\n"), vec![vec![span("99%", Color::Default, false)]]);
    }

    #[test]
    fn tab_expands_to_multiple_of_four() {
        assert_eq!(parse_lines(b"ab\tc\n"), vec![vec![span("ab  c", Color::Default, false)]]);
        assert_eq!(parse_lines(b"\tx\n"), vec![vec![span("    x", Color::Default, false)]]);
    }

    #[test]
    fn bold_bright_and_default() {
        let lines = parse_lines(b"\x1b[1;92mA\x1b[22mB\x1b[39mC\x1b[mD");
        assert_eq!(
            lines,
            vec![vec![
                span("A", Color::Ansi(10), true),
                span("B", Color::Ansi(10), false),
                span("CD", Color::Default, false),
            ]]
        );
    }

    #[test]
    fn extended_colors() {
        let lines = parse_lines(b"\x1b[38;5;4ma\x1b[38;5;196mb\x1b[38;2;1;2;3mc\x1b[38:5:46md");
        assert_eq!(
            lines,
            vec![vec![
                span("a", Color::Ansi(4), false),
                span("b", Color::Ansi(9), false),
                span("c", Color::Rgb(1, 2, 3), false),
                span("d", Color::Ansi(10), false),
            ]]
        );
    }

    #[test]
    fn background_params_do_not_leak_into_fg() {
        let lines = parse_lines(b"\x1b[48;5;31mx\x1b[48;2;31;32;33my");
        assert_eq!(lines, vec![vec![span("xy", Color::Default, false)]]);
    }

    #[test]
    fn invalid_utf8_is_replaced() {
        let lines = parse_lines(b"a\xffb\n");
        assert_eq!(lines, vec![vec![span("a\u{FFFD}b", Color::Default, false)]]);
    }
}
