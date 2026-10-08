use crate::sys;

pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng {
            state: seed ^ 0x9e3779b97f4a7c15,
        }
    }

    pub fn from_entropy() -> Rng {
        let mut buf = [0u8; 16];
        let mut seed = 0x243f6a8885a308d3u64;
        if sys::getrandom(&mut buf).is_ok() {
            let mut a = [0u8; 8];
            let mut b = [0u8; 8];
            a.copy_from_slice(&buf[..8]);
            b.copy_from_slice(&buf[8..]);
            seed ^= u64::from_ne_bytes(a).rotate_left(17);
            seed = seed.wrapping_mul(0x100000001b3) ^ u64::from_ne_bytes(b);
        }
        seed ^= crate::time::realtime_ns();
        Rng::new(seed)
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    #[inline]
    pub fn next_bounded(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u64();
            if r >= threshold {
                return r % bound;
            }
        }
    }

    pub fn fill_bytes(&mut self, out: &mut [u8]) {
        let mut i = 0;
        while i + 8 <= out.len() {
            let v = self.next_u64().to_ne_bytes();
            out[i..i + 8].copy_from_slice(&v);
            i += 8;
        }
        if i < out.len() {
            let v = self.next_u64().to_ne_bytes();
            let n = out.len() - i;
            out[i..].copy_from_slice(&v[..n]);
        }
    }

    /// Fisher-Yates shuffle.
    pub fn shuffle_u32(&mut self, data: &mut [u32]) {
        let mut i = data.len();
        while i > 1 {
            i -= 1;
            let j = self.next_bounded(i as u64 + 1) as usize;
            data.swap(i, j);
        }
    }

    pub fn shuffle_u64(&mut self, data: &mut [u64]) {
        let mut i = data.len();
        while i > 1 {
            i -= 1;
            let j = self.next_bounded(i as u64 + 1) as usize;
            data.swap(i, j);
        }
    }
}
