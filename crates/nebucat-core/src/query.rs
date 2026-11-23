#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Apps(String),
    Command(String),
    Files(String),
    Plugin { keyword: String, rest: String },
    System(String),
}

pub fn parse(input: &str, plugin_keywords: &[String], system: bool) -> Mode {
    if let Some(rest) = input.strip_prefix('>') {
        return Mode::Command(rest.trim_start().to_string());
    }
    if let Some(rest) = input.strip_prefix('/') {
        return Mode::Files(rest.trim_start().to_string());
    }
    if system && let Some(rest) = input.strip_prefix('!') {
        return Mode::System(rest.trim().to_string());
    }
    let trimmed = input.trim_start();
    if let Some(end) = trimmed.find(char::is_whitespace) {
        let word = &trimmed[..end];
        let after = &trimmed[end..];
        if let Some(rest) = after.strip_prefix(' ')
            && plugin_keywords.iter().any(|k| k == word)
        {
            return Mode::Plugin {
                keyword: word.to_string(),
                rest: rest.trim_start().to_string(),
            };
        }
    }
    Mode::Apps(input.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_trims_leading_space() {
        assert_eq!(parse(">  dir", &[], true), Mode::Command("dir".into()));
    }

    #[test]
    fn files_prefix() {
        assert_eq!(parse("/ notes", &[], true), Mode::Files("notes".into()));
    }

    #[test]
    fn system_prefix() {
        assert_eq!(parse("! reb", &[], true), Mode::System("reb".into()));
        assert_eq!(parse("!", &[], true), Mode::System(String::new()));
    }

    #[test]
    fn system_prefix_off_is_apps() {
        assert_eq!(parse("!reb", &[], false), Mode::Apps("!reb".into()));
    }

    #[test]
    fn plugin_keyword_with_space() {
        assert_eq!(
            parse("hi there", &["hi".into()], true),
            Mode::Plugin {
                keyword: "hi".into(),
                rest: "there".into()
            }
        );
    }

    #[test]
    fn plugin_keyword_trailing_space_gives_empty_rest() {
        assert_eq!(
            parse("hi ", &["hi".into()], true),
            Mode::Plugin {
                keyword: "hi".into(),
                rest: String::new()
            }
        );
    }

    #[test]
    fn plugin_keyword_alone_is_apps() {
        assert_eq!(parse("hi", &["hi".into()], true), Mode::Apps("hi".into()));
    }

    #[test]
    fn unknown_word_is_apps() {
        assert_eq!(
            parse("  hello world ", &["hi".into()], true),
            Mode::Apps("hello world".into())
        );
    }
}
