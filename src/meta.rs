//! Validation for the `--meta` CLI argument.
//!
//! The argument must be a well-formed JSON object. It is copied into the
//! report verbatim, so the validator only needs to guarantee that the text is
//! a syntactically valid JSON value of the expected shape (object, bounded
//! size, bounded nesting, no duplicate top-level keys).

pub const MAX_META_BYTES: usize = 4096;
pub const MAX_META_KEYS: usize = 32;
const MAX_DEPTH: usize = 16;

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, c: u8, what: &'static str) -> Result<(), &'static str> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(what)
        }
    }

    fn parse_string(&mut self) -> Result<(), &'static str> {
        self.expect(b'"', "meta: expected string")?;
        loop {
            match self.peek() {
                None => return Err("meta: unterminated string"),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(());
                }
                Some(b'\\') => {
                    self.i += 1;
                    match self.peek() {
                        Some(b'"') | Some(b'\\') | Some(b'/') | Some(b'b') | Some(b'f')
                        | Some(b'n') | Some(b'r') | Some(b't') => {
                            self.i += 1;
                        }
                        Some(b'u') => {
                            self.i += 1;
                            for _ in 0..4 {
                                match self.peek() {
                                    Some(c) if c.is_ascii_hexdigit() => self.i += 1,
                                    _ => return Err("meta: invalid \\u escape"),
                                }
                            }
                        }
                        _ => return Err("meta: invalid escape"),
                    }
                }
                Some(c) if c < 0x20 => return Err("meta: control character in string"),
                Some(_) => self.i += 1,
            }
        }
    }

    fn parse_number(&mut self) -> Result<(), &'static str> {
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.i += 1;
                }
            }
            _ => return Err("meta: invalid number"),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err("meta: invalid number");
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.i += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err("meta: invalid number");
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        Ok(())
    }

    fn parse_literal(&mut self, lit: &[u8]) -> Result<(), &'static str> {
        if self.b.len() - self.i >= lit.len() && &self.b[self.i..self.i + lit.len()] == lit {
            self.i += lit.len();
            Ok(())
        } else {
            Err("meta: invalid literal")
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<(), &'static str> {
        if depth > MAX_DEPTH {
            return Err("meta: nesting too deep");
        }
        self.ws();
        match self.peek() {
            Some(b'{') => self.parse_object(depth),
            Some(b'[') => self.parse_array(depth),
            Some(b'"') => self.parse_string(),
            Some(b't') => self.parse_literal(b"true"),
            Some(b'f') => self.parse_literal(b"false"),
            Some(b'n') => self.parse_literal(b"null"),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            _ => Err("meta: invalid value"),
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<(), &'static str> {
        self.expect(b'{', "meta: expected '{'")?;
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.ws();
            self.parse_string()?;
            self.ws();
            self.expect(b':', "meta: expected ':'")?;
            self.parse_value(depth + 1)?;
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => return Err("meta: expected ',' or '}'"),
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<(), &'static str> {
        self.expect(b'[', "meta: expected '['")?;
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.parse_value(depth + 1)?;
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => return Err("meta: expected ',' or ']'"),
            }
        }
    }

    fn parse_top_object(&mut self) -> Result<(), &'static str> {
        self.expect(b'{', "meta: must be a JSON object")?;
        self.ws();
        let mut keys: [&[u8]; MAX_META_KEYS] = [b""; MAX_META_KEYS];
        let mut n = 0usize;
        if self.peek() == Some(b'}') {
            self.i += 1;
        } else {
            loop {
                self.ws();
                let key_start = self.i;
                self.parse_string()?;
                let key = &self.b[key_start..self.i];
                for seen in keys.iter().take(n) {
                    if *seen == key {
                        return Err("meta: duplicate key");
                    }
                }
                if n >= MAX_META_KEYS {
                    return Err("meta: too many keys (max 32)");
                }
                keys[n] = key;
                n += 1;
                self.ws();
                self.expect(b':', "meta: expected ':'")?;
                self.parse_value(1)?;
                self.ws();
                match self.peek() {
                    Some(b',') => self.i += 1,
                    Some(b'}') => {
                        self.i += 1;
                        break;
                    }
                    _ => return Err("meta: expected ',' or '}'"),
                }
            }
        }
        self.ws();
        if self.i != self.b.len() {
            return Err("meta: trailing data after object");
        }
        Ok(())
    }
}

/// Checks that `s` is a JSON object of at most `MAX_META_BYTES` bytes with at
/// most `MAX_META_KEYS` top-level keys and no duplicate keys.
pub fn validate_object(s: &str) -> Result<(), &'static str> {
    if s.len() > MAX_META_BYTES {
        return Err("meta: too long (max 4096 bytes)");
    }
    let mut p = Parser {
        b: s.as_bytes(),
        i: 0,
    };
    p.ws();
    p.parse_top_object()
}
