use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Key,
    Str,
    Num,
    /// `true`, `false` or `null`.
    Lit,
    Punct,
    Ws,
}

pub fn pretty(input: &str) -> Option<Vec<Vec<(Tok, String)>>> {
    let input = input.trim();
    if !(input.starts_with('{') || input.starts_with('[')) {
        return None;
    }
    let value: Value = serde_json::from_str(input).ok()?;
    let text = serde_json::to_string_pretty(&value).ok()?;
    Some(text.lines().map(tokenize_line).collect())
}

fn tokenize_line(line: &str) -> Vec<(Tok, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        let tok = match c {
            ' ' | '\t' => {
                while i < chars.len() && matches!(chars[i], ' ' | '\t') {
                    i += 1;
                }
                Tok::Ws
            }
            '"' => {
                i += 1;
                while i < chars.len() {
                    match chars[i] {
                        '\\' => i += 2,
                        '"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                i = i.min(chars.len());
                let mut j = i;
                while j < chars.len() && matches!(chars[j], ' ' | '\t') {
                    j += 1;
                }
                if chars.get(j) == Some(&':') {
                    Tok::Key
                } else {
                    Tok::Str
                }
            }
            '{' | '}' | '[' | ']' | ':' | ',' => {
                i += 1;
                Tok::Punct
            }
            c if c == '-' || c.is_ascii_digit() => {
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-'))
                {
                    i += 1;
                }
                Tok::Num
            }
            c if c.is_ascii_alphabetic() => {
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                Tok::Lit
            }
            _ => {
                i += 1;
                Tok::Punct
            }
        };
        out.push((tok, chars[start..i].iter().collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(tok: Tok, s: &str) -> (Tok, String) {
        (tok, s.to_string())
    }

    #[test]
    fn first_line_is_open_brace() {
        let lines = pretty("{\"a\":[1,true]}").unwrap();
        assert_eq!(lines[0], vec![t(Tok::Punct, "{")]);
    }

    #[test]
    fn tokenizes_pretty_lines() {
        let lines = pretty("{\"a\":[1,true]}").unwrap();
        assert_eq!(
            lines,
            vec![
                vec![t(Tok::Punct, "{")],
                vec![
                    t(Tok::Ws, "  "),
                    t(Tok::Key, "\"a\""),
                    t(Tok::Punct, ":"),
                    t(Tok::Ws, " "),
                    t(Tok::Punct, "["),
                ],
                vec![t(Tok::Ws, "    "), t(Tok::Num, "1"), t(Tok::Punct, ",")],
                vec![t(Tok::Ws, "    "), t(Tok::Lit, "true")],
                vec![t(Tok::Ws, "  "), t(Tok::Punct, "]")],
                vec![t(Tok::Punct, "}")],
            ]
        );
    }

    #[test]
    fn string_values_and_escapes() {
        let lines = pretty(r#"  {"k": "a \"q\" b", "n": -1.5e3, "z": null}  "#).unwrap();
        assert_eq!(
            lines[1],
            vec![
                t(Tok::Ws, "  "),
                t(Tok::Key, "\"k\""),
                t(Tok::Punct, ":"),
                t(Tok::Ws, " "),
                t(Tok::Str, r#""a \"q\" b""#),
                t(Tok::Punct, ","),
            ]
        );
        assert_eq!(lines[2][4], t(Tok::Num, "-1500.0"));
        assert_eq!(lines[3][4], t(Tok::Lit, "null"));
    }

    #[test]
    fn rejects_non_container_and_invalid() {
        assert_eq!(pretty("hello"), None);
        assert_eq!(pretty("42"), None);
        assert_eq!(pretty("{\"a\":"), None);
    }
}
