/// `vk == 0` means a lone modifier (e.g. "Win") that fires when tapped on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub vk: u32,
}

impl Hotkey {
    pub fn is_modifier_only(&self) -> bool {
        self.vk == 0
    }
}

pub fn parse(s: &str) -> Result<Hotkey, String> {
    let err = |reason: String| format!("hotkey \"{s}\": {reason}");
    let mut hk = Hotkey { ctrl: false, alt: false, shift: false, win: false, vk: 0 };
    let mut key: Option<u32> = None;
    for part in s.split('+').map(str::trim) {
        if part.is_empty() {
            return Err(err("empty part".into()));
        }
        let lower = part.to_ascii_lowercase();
        let modifier = match lower.as_str() {
            "ctrl" | "control" => Some(&mut hk.ctrl),
            "alt" => Some(&mut hk.alt),
            "shift" => Some(&mut hk.shift),
            "win" => Some(&mut hk.win),
            _ => None,
        };
        if let Some(flag) = modifier {
            if *flag {
                return Err(err(format!("{part} appears twice")));
            }
            *flag = true;
            continue;
        }
        let vk = key_code(&lower).ok_or_else(|| err(format!("unknown key \"{part}\"")))?;
        if key.is_some() {
            return Err(err("only one key allowed".into()));
        }
        key = Some(vk);
    }
    match key {
        Some(vk) => hk.vk = vk,
        None => {
            let count = [hk.ctrl, hk.alt, hk.shift, hk.win].iter().filter(|m| **m).count();
            if count != 1 {
                return Err(err("missing a key".into()));
            }
        }
    }
    Ok(hk)
}

fn key_code(lower: &str) -> Option<u32> {
    if lower == "space" {
        return Some(0x20);
    }
    let bytes = lower.as_bytes();
    if bytes.len() == 1 {
        let c = bytes[0];
        return match c {
            b'a'..=b'z' => Some(c.to_ascii_uppercase() as u32),
            b'0'..=b'9' => Some(c as u32),
            _ => None,
        };
    }
    let n: u32 = lower.strip_prefix('f')?.parse().ok()?;
    (1..=12).contains(&n).then(|| 0x70 + n - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_space() {
        let hk = parse("alt+space").unwrap();
        assert_eq!(hk, Hotkey { ctrl: false, alt: true, shift: false, win: false, vk: 0x20 });
    }

    #[test]
    fn bare_key_is_allowed() {
        let hk = parse("F9").unwrap();
        assert_eq!(hk, Hotkey { ctrl: false, alt: false, shift: false, win: false, vk: 0x78 });
        assert!(!hk.is_modifier_only());
    }

    #[test]
    fn lone_modifier() {
        let hk = parse("Win").unwrap();
        assert!(hk.win && !hk.ctrl && !hk.alt && !hk.shift);
        assert!(hk.is_modifier_only());
        assert!(parse("alt").unwrap().alt);
        assert!(parse("Win+Shift").is_err());
    }

    #[test]
    fn function_key() {
        assert_eq!(parse("Ctrl+F5").unwrap().vk, 0x74);
        assert_eq!(parse("Ctrl+F1").unwrap().vk, 0x70);
        assert_eq!(parse("Ctrl+F12").unwrap().vk, 0x7B);
        assert!(parse("Ctrl+F13").is_err());
    }

    #[test]
    fn letters_digits_and_control() {
        let hk = parse("Control+Alt+k").unwrap();
        assert!(hk.ctrl && hk.alt && !hk.shift && !hk.win);
        assert_eq!(hk.vk, 'K' as u32);
        assert_eq!(parse("WIN+SHIFT+7").unwrap().vk, '7' as u32);
    }

    #[test]
    fn invalid_inputs() {
        assert!(parse("Ctrl+A+B").is_err());
        assert!(parse("Ctrl+Alt").is_err());
        assert!(parse("Ctrl+Foo").is_err());
        assert!(parse("Ctrl++A").is_err());
        assert!(parse("Ctrl+Ctrl+A").is_err());
        assert!(parse("").is_err());
    }
}
