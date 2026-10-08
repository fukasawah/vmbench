use crate::fmt;

const MAX_DEPTH: usize = 64;

/// Streaming JSON writer with automatic member/element separators.
///
/// Call `begin_obj`/`begin_arr`/`end` for containers, `elem` before array
/// elements, and the `key*` helpers for object members. Value methods write
/// raw JSON values and never insert separators themselves.
pub struct Out {
    buf: *mut u8,
    len: usize,
    cap: usize,
    kinds: [u8; MAX_DEPTH],
    has: [bool; MAX_DEPTH],
    depth: usize,
}

impl Out {
    pub unsafe fn new(buf: *mut u8, cap: usize) -> Out {
        Out {
            buf,
            len: 0,
            cap,
            kinds: [0; MAX_DEPTH],
            has: [false; MAX_DEPTH],
            depth: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn put(&mut self, c: u8) {
        if self.len >= self.cap {
            crate::rt::fatal("output buffer exhausted");
        }
        unsafe { *self.buf.add(self.len) = c };
        self.len += 1;
    }

    pub fn raw(&mut self, b: &[u8]) {
        for &c in b {
            self.put(c);
        }
    }

    pub fn ch(&mut self, c: u8) {
        self.put(c);
    }

    pub fn str(&mut self, s: &str) {
        self.raw(s.as_bytes());
    }

    pub fn begin_obj(&mut self) {
        self.put(b'{');
        if self.depth < MAX_DEPTH {
            self.kinds[self.depth] = b'{';
            self.has[self.depth] = false;
            self.depth += 1;
        }
    }

    pub fn begin_arr(&mut self) {
        self.put(b'[');
        if self.depth < MAX_DEPTH {
            self.kinds[self.depth] = b'[';
            self.has[self.depth] = false;
            self.depth += 1;
        }
    }

    pub fn end(&mut self) {
        if self.depth == 0 {
            crate::rt::fatal("json: unbalanced end()");
        }
        self.depth -= 1;
        let close = if self.kinds[self.depth] == b'{' { b'}' } else { b']' };
        self.put(close);
    }

    /// Inserts a separator if needed for the next array element.
    pub fn elem(&mut self) {
        if self.depth > 0 && self.kinds[self.depth - 1] == b'[' {
            if self.has[self.depth - 1] {
                self.put(b',');
            }
            self.has[self.depth - 1] = true;
        }
    }

    fn member(&mut self) {
        if self.depth > 0 && self.kinds[self.depth - 1] == b'{' {
            if self.has[self.depth - 1] {
                self.put(b',');
            }
            self.has[self.depth - 1] = true;
        }
    }

    pub fn jstr(&mut self, s: &str) {
        self.put(b'"');
        for &b in s.as_bytes() {
            match b {
                b'"' => self.raw(b"\\\""),
                b'\\' => self.raw(b"\\\\"),
                b'\n' => self.raw(b"\\n"),
                b'\r' => self.raw(b"\\r"),
                b'\t' => self.raw(b"\\t"),
                0x00..=0x1f => {
                    let mut tmp = [0u8; 6];
                    tmp[0] = b'\\';
                    tmp[1] = b'u';
                    tmp[2] = b'0';
                    tmp[3] = b'0';
                    let mut d = [0u8; 2];
                    let _ = fmt::hex_lower(&[b], &mut d);
                    tmp[4] = d[0];
                    tmp[5] = d[1];
                    self.raw(&tmp);
                }
                _ => self.put(b),
            }
        }
        self.put(b'"');
    }

    pub fn key(&mut self, k: &str) {
        self.member();
        self.jstr(k);
        self.put(b':');
    }

    pub fn u64(&mut self, v: u64) {
        let mut tmp = [0u8; 20];
        let n = fmt::u64_dec(v, &mut tmp);
        self.raw(&tmp[..n]);
    }

    pub fn i64(&mut self, v: i64) {
        let mut tmp = [0u8; 24];
        let n = fmt::i64_dec(v, &mut tmp);
        self.raw(&tmp[..n]);
    }

    pub fn usize(&mut self, v: usize) {
        self.u64(v as u64);
    }

    pub fn f64(&mut self, v: f64) {
        let mut tmp = [0u8; 32];
        let n = fmt::f64_auto(v, &mut tmp);
        self.raw(&tmp[..n]);
    }

    pub fn f64_fixed(&mut self, v: f64, decimals: u32) {
        let mut tmp = [0u8; 40];
        let n = fmt::f64_fixed(v, decimals, &mut tmp);
        self.raw(&tmp[..n]);
    }

    pub fn bool(&mut self, b: bool) {
        self.raw(if b { b"true" } else { b"false" });
    }

    pub fn null(&mut self) {
        self.raw(b"null");
    }

    pub fn sha256(&mut self, hash: &[u8; 32]) {
        self.str("\"sha256:");
        let mut tmp = [0u8; 64];
        let n = fmt::hex_lower(hash, &mut tmp);
        self.raw(&tmp[..n]);
        self.put(b'"');
    }

    pub fn key_u64(&mut self, k: &str, v: u64) {
        self.key(k);
        self.u64(v);
    }

    pub fn key_i64(&mut self, k: &str, v: i64) {
        self.key(k);
        self.i64(v);
    }

    pub fn key_usize(&mut self, k: &str, v: usize) {
        self.key(k);
        self.u64(v as u64);
    }

    pub fn key_f64(&mut self, k: &str, v: f64) {
        self.key(k);
        self.f64(v);
    }

    pub fn key_bool(&mut self, k: &str, v: bool) {
        self.key(k);
        self.bool(v);
    }

    pub fn key_str(&mut self, k: &str, v: &str) {
        self.key(k);
        self.jstr(v);
    }

    pub fn key_null(&mut self, k: &str) {
        self.key(k);
        self.null();
    }

    pub fn key_opt_str(&mut self, k: &str, v: Option<&str>) {
        self.key(k);
        match v {
            Some(s) => self.jstr(s),
            None => self.null(),
        }
    }

    pub fn key_opt_u64(&mut self, k: &str, v: Option<u64>) {
        self.key(k);
        match v {
            Some(n) => self.u64(n),
            None => self.null(),
        }
    }

    pub fn key_opt_f64(&mut self, k: &str, v: Option<f64>) {
        self.key(k);
        match v {
            Some(n) => self.f64(n),
            None => self.null(),
        }
    }

    pub fn key_opt_bool(&mut self, k: &str, v: Option<bool>) {
        self.key(k);
        match v {
            Some(b) => self.bool(b),
            None => self.null(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.buf, self.len) }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.bytes()
    }
}
