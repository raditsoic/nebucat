pub fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0F) as usize] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_and_ampersand() {
        assert_eq!(encode_query("a b&c"), "a+b%26c");
    }

    #[test]
    fn unreserved_kept_and_utf8_encoded() {
        assert_eq!(encode_query("A-z_0.9~"), "A-z_0.9~");
        assert_eq!(encode_query("é"), "%C3%A9");
    }
}
