pub fn eval(input: &str) -> Option<f64> {
    if !input.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    if !input
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_whitespace() || "+-*/%^().".contains(c))
    {
        return None;
    }
    let mut p = Parser { src: input.as_bytes(), pos: 0, binary: false };
    let v = p.expr()?;
    p.skip_ws();
    if p.pos != p.src.len() || !p.binary || !v.is_finite() {
        return None;
    }
    Some(v)
}

pub fn format(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    if v.fract() == 0.0 {
        return format!("{v}");
    }
    let s = format!("{v:.10}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    binary: bool,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.src.get(self.pos).copied()
    }

    fn binop(&mut self, v: f64) -> Option<f64> {
        self.binary = true;
        v.is_finite().then_some(v)
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(op @ (b'+' | b'-')) = self.peek() {
            self.pos += 1;
            let r = self.term()?;
            v = self.binop(if op == b'+' { v + r } else { v - r })?;
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while let Some(op @ (b'*' | b'/' | b'%')) = self.peek() {
            self.pos += 1;
            let r = self.unary()?;
            v = self.binop(match op {
                b'*' => v * r,
                b'/' => v / r,
                _ => v % r,
            })?;
        }
        Some(v)
    }

    fn unary(&mut self) -> Option<f64> {
        if self.peek() == Some(b'-') {
            self.pos += 1;
            return Some(-self.unary()?);
        }
        self.power()
    }

    // Exponent goes through unary so `2^-1` parses; `^` still binds tighter than a leading minus.
    fn power(&mut self) -> Option<f64> {
        let base = self.primary()?;
        if self.peek() == Some(b'^') {
            self.pos += 1;
            let exp = self.unary()?;
            return self.binop(base.powf(exp));
        }
        Some(base)
    }

    fn primary(&mut self) -> Option<f64> {
        match self.peek()? {
            b'(' => {
                self.pos += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') {
                    return None;
                }
                self.pos += 1;
                Some(v)
            }
            b'0'..=b'9' | b'.' => {
                let start = self.pos;
                while self.pos < self.src.len()
                    && (self.src[self.pos].is_ascii_digit() || self.src[self.pos] == b'.')
                {
                    self.pos += 1;
                }
                std::str::from_utf8(&self.src[start..self.pos]).ok()?.parse().ok()
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_is_float() {
        assert_eq!(eval("7/2"), Some(3.5));
    }

    #[test]
    fn power_is_right_associative() {
        assert_eq!(eval("2^3^2"), Some(512.0));
    }

    #[test]
    fn unary_minus_and_parens() {
        assert_eq!(eval("-(2+3)*4"), Some(-20.0));
    }

    #[test]
    fn power_binds_tighter_than_unary_minus() {
        assert_eq!(eval("-2^2"), Some(-4.0));
        assert_eq!(eval("2^-1"), Some(0.5));
    }

    #[test]
    fn precedence_and_whitespace() {
        assert_eq!(eval(" 1 + 2 * 3 "), Some(7.0));
        assert_eq!(eval("10 % 4"), Some(2.0));
        assert_eq!(eval("1.5*2"), Some(3.0));
    }

    #[test]
    fn rejects_non_expressions() {
        assert_eq!(eval("42"), None);
        assert_eq!(eval("-5"), None);
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("(1+2"), None);
        assert_eq!(eval("1+2)"), None);
        assert_eq!(eval("1 2+3"), None);
        assert_eq!(eval("1.2.3+1"), None);
        assert_eq!(eval("code"), None);
        assert_eq!(eval("2x+1"), None);
        assert_eq!(eval("+"), None);
    }

    #[test]
    fn format_values() {
        assert_eq!(format(3.0), "3");
        assert_eq!(format(0.1 + 0.2), "0.3");
        assert_eq!(format(-0.0), "0");
        assert_eq!(format(-20.0), "-20");
        assert_eq!(format(10.5), "10.5");
        assert_eq!(format(1.0 / 3.0), "0.3333333333");
    }
}
