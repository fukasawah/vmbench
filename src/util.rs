use crate::sys;

pub struct Arena {
    base: usize,
    cur: usize,
    end: usize,
}

impl Arena {
    pub fn new(bytes: usize) -> Option<Arena> {
        let p = sys::mmap_anon(bytes, false)?;
        Some(Arena {
            base: p as usize,
            cur: p as usize,
            end: p as usize + bytes,
        })
    }

    pub fn used(&self) -> usize {
        self.cur - self.base
    }

    pub fn capacity(&self) -> usize {
        self.end - self.base
    }

    pub unsafe fn alloc_raw(&mut self, n: usize, align: usize) -> Option<*mut u8> {
        let a = align.max(1);
        let start = (self.cur + a - 1) & !(a - 1);
        let end = start.checked_add(n)?;
        if end > self.end {
            return None;
        }
        self.cur = end;
        Some(start as *mut u8)
    }

    pub unsafe fn alloc_bytes(&mut self, n: usize, align: usize) -> Option<&'static mut [u8]> {
        let p = self.alloc_raw(n, align)?;
        core::ptr::write_bytes(p, 0, n);
        Some(core::slice::from_raw_parts_mut(p, n))
    }

    pub unsafe fn alloc_slice<T: Copy + 'static>(
        &mut self,
        n: usize,
    ) -> Option<&'static mut [T]> {
        let bytes = n.checked_mul(core::mem::size_of::<T>())?;
        let p = self.alloc_raw(bytes, core::mem::align_of::<T>())? as *mut T;
        core::ptr::write_bytes(p as *mut u8, 0, bytes);
        Some(core::slice::from_raw_parts_mut(p, n))
    }

    pub unsafe fn boxed<T>(&mut self, val: T) -> Option<&'static mut T> {
        let p = self.alloc_raw(core::mem::size_of::<T>(), core::mem::align_of::<T>())? as *mut T;
        core::ptr::write(p, val);
        Some(&mut *p)
    }

    pub unsafe fn copy_str(&mut self, s: &[u8]) -> Option<&'static str> {
        let p = self.alloc_bytes(s.len(), 1)?;
        core::ptr::copy_nonoverlapping(s.as_ptr(), p.as_mut_ptr(), s.len());
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(p.as_ptr(), s.len())).into()
    }
}

pub fn bytes_to_str(b: &[u8]) -> &str {
    core::str::from_utf8(b).unwrap_or("")
}

pub fn trim_ascii(b: &[u8]) -> &[u8] {
    let mut s = 0;
    let mut e = b.len();
    while s < e && (b[s] == b' ' || b[s] == b'\t' || b[s] == b'\n' || b[s] == b'\r') {
        s += 1;
    }
    while e > s && (b[e - 1] == b' ' || b[e - 1] == b'\t' || b[e - 1] == b'\n' || b[e - 1] == b'\r')
    {
        e -= 1;
    }
    &b[s..e]
}

pub fn parse_u64(b: &[u8]) -> Option<u64> {
    let b = trim_ascii(b);
    if b.is_empty() {
        return None;
    }
    let mut v: u64 = 0;
    for &c in b {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v.checked_mul(10)?.checked_add((c - b'0') as u64)?;
    }
    Some(v)
}

pub fn parse_i64(b: &[u8]) -> Option<i64> {
    let b = trim_ascii(b);
    if b.is_empty() {
        return None;
    }
    let (neg, digits) = if b[0] == b'-' {
        (true, &b[1..])
    } else if b[0] == b'+' {
        (false, &b[1..])
    } else {
        (false, b)
    };
    let v = parse_u64(digits)?;
    Some(if neg {
        -(v as i64)
    } else {
        v as i64
    })
}

pub fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    let first = needle[0];
    let last = hay.len() - needle.len();
    let mut i = 0;
    while i <= last {
        if hay[i] == first && &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn nth_field<'a>(line: &'a [u8], sep: u8, n: usize) -> Option<&'a [u8]> {
    let mut count = 0;
    let mut start = 0;
    let mut i = 0;
    while i <= line.len() {
        if i == line.len() || line[i] == sep {
            if count == n {
                return Some(trim_ascii(&line[start..i]));
            }
            count += 1;
            start = i + 1;
        }
        i += 1;
    }
    None
}

pub fn eq_ignore_case(a: &[u8], b: &str) -> bool {
    let b = b.as_bytes();
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.to_ascii_lowercase() == y.to_ascii_lowercase())
}
