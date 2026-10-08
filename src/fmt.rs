pub fn u64_dec(v: u64, out: &mut [u8]) -> usize {
    let mut tmp = [0u8; 20];
    let mut n = 0;
    let mut v = v;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
        if v == 0 {
            break;
        }
    }
    for i in 0..n {
        out[i] = tmp[n - 1 - i];
    }
    n
}

pub fn i64_dec(v: i64, out: &mut [u8]) -> usize {
    if v < 0 {
        out[0] = b'-';
        1 + u64_dec((v as u64).wrapping_neg(), &mut out[1..])
    } else {
        u64_dec(v as u64, out)
    }
}

pub fn hex_lower(bytes: &[u8], out: &mut [u8]) -> usize {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let n = bytes.len() * 2;
    for (i, &b) in bytes.iter().enumerate() {
        out[i * 2] = HEX[(b >> 4) as usize];
        out[i * 2 + 1] = HEX[(b & 0xf) as usize];
    }
    n
}

pub fn f64_fixed(v: f64, decimals: u32, out: &mut [u8]) -> usize {
    if v.is_nan() {
        out[..4].copy_from_slice(b"null");
        return 4;
    }
    if v.is_infinite() {
        if v > 0.0 {
            out[..4].copy_from_slice(b"1e30");
        } else {
            out[..5].copy_from_slice(b"-1e30");
            return 5;
        }
        return 4;
    }
    let mut pos = 0;
    let mut v = v;
    if v < 0.0 {
        out[pos] = b'-';
        pos += 1;
        v = -v;
    }
    let mut scale: u64 = 1;
    for _ in 0..decimals {
        scale *= 10;
    }
    let scaled = v * scale as f64;
    let rounded = if scaled >= 9.22e18 {
        u64::MAX
    } else {
        (scaled + 0.5) as u64
    };
    if decimals == 0 {
        let mut tmp = [0u8; 20];
        let n = u64_dec(rounded, &mut tmp);
        out[pos..pos + n].copy_from_slice(&tmp[..n]);
        return pos + n;
    }
    let int_part = rounded / scale;
    let frac_part = rounded % scale;
    let mut tmp = [0u8; 20];
    let n = u64_dec(int_part, &mut tmp);
    out[pos..pos + n].copy_from_slice(&tmp[..n]);
    pos += n;
    out[pos] = b'.';
    pos += 1;
    let mut ftmp = [0u8; 20];
    let fn_ = u64_dec(frac_part, &mut ftmp);
    let d = decimals as usize;
    for i in 0..d {
        out[pos + i] = if i + fn_ < d { b'0' } else { ftmp[i + fn_ - d] };
    }
    pos + d
}

pub fn f64_auto(v: f64, out: &mut [u8]) -> usize {
    let a = if v < 0.0 { -v } else { v };
    if !a.is_finite() {
        return f64_fixed(v, 0, out);
    }
    let decimals = if a == 0.0 {
        3
    } else if a < 10.0 {
        3
    } else if a < 1000.0 {
        2
    } else if a < 1_000_000.0 {
        0
    } else {
        0
    };
    f64_fixed(v, decimals, out)
}

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Formats epoch nanoseconds as an ISO-8601 UTC timestamp.
pub fn iso8601(epoch_ns: u64, out: &mut [u8]) -> usize {
    let secs = (epoch_ns / 1_000_000_000) as i64;
    let nanos = (epoch_ns % 1_000_000_000) as u32;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    let h = (rem / 3600) as u32;
    let mi = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;
    let mut pos = 0;
    out[pos] = b'0' + (y / 1000) as u8;
    out[pos + 1] = b'0' + ((y / 100) % 10) as u8;
    out[pos + 2] = b'0' + ((y / 10) % 10) as u8;
    out[pos + 3] = b'0' + (y % 10) as u8;
    pos += 4;
    out[pos] = b'-';
    pos += 1;
    pos += two_digit(mo, &mut out[pos..]);
    out[pos] = b'-';
    pos += 1;
    pos += two_digit(d, &mut out[pos..]);
    out[pos] = b'T';
    pos += 1;
    pos += two_digit(h, &mut out[pos..]);
    out[pos] = b':';
    pos += 1;
    pos += two_digit(mi, &mut out[pos..]);
    out[pos] = b':';
    pos += 1;
    pos += two_digit(s, &mut out[pos..]);
    out[pos] = b'.';
    pos += 1;
    let mut tmp = [0u8; 12];
    let n = u64_dec(nanos as u64, &mut tmp);
    for i in 0..9 {
        out[pos + i] = if i + n < 9 { b'0' } else { tmp[i + n - 9] };
    }
    pos += 9;
    out[pos] = b'Z';
    pos + 1
}

fn two_digit(v: u32, out: &mut [u8]) -> usize {
    out[0] = b'0' + (v / 10) as u8;
    out[1] = b'0' + (v % 10) as u8;
    2
}
