#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    HigherBetter,
    LowerBetter,
    Neutral,
}

#[derive(Clone, Copy)]
pub struct ScalarStats {
    pub n: usize,
    pub median: f64,
    pub mean: f64,
    pub best: f64,
    pub worst: f64,
    pub min: f64,
    pub max: f64,
    pub variation_pct: f64,
}

pub fn compute(values: &[f64], dir: Direction) -> ScalarStats {
    let n = values.len();
    if n == 0 {
        return ScalarStats {
            n: 0,
            median: 0.0,
            mean: 0.0,
            best: 0.0,
            worst: 0.0,
            min: 0.0,
            max: 0.0,
            variation_pct: 0.0,
        };
    }
    let mut sorted = [0f64; 16];
    let m = n.min(16);
    sorted[..m].copy_from_slice(&values[..m]);
    insertion_sort(&mut sorted[..m]);
    let median = if m % 2 == 1 {
        sorted[m / 2]
    } else {
        (sorted[m / 2 - 1] + sorted[m / 2]) / 2.0
    };
    let mut sum = 0.0;
    for &v in &sorted[..m] {
        sum += v;
    }
    let mean = sum / m as f64;
    let min = sorted[0];
    let max = sorted[m - 1];
    let (best, worst) = match dir {
        Direction::HigherBetter => (max, min),
        Direction::LowerBetter => (min, max),
        Direction::Neutral => (median, median),
    };
    let variation_pct = if median != 0.0 {
        (max - min) / median.abs() * 100.0
    } else {
        0.0
    };
    ScalarStats {
        n: m,
        median,
        mean,
        best,
        worst,
        min,
        max,
        variation_pct,
    }
}

fn insertion_sort(a: &mut [f64]) {
    for i in 1..a.len() {
        let v = a[i];
        let mut j = i;
        while j > 0 && a[j - 1] > v {
            a[j] = a[j - 1];
            j -= 1;
        }
        a[j] = v;
    }
}


