//! Pure-Rust statistical core for duckdb_spc v0.1.0.
//!
//! Faithful port of the validated Python/scipy oracle
//! (`test/oracle.py`). All functions are closed-form (no convergence loops):
//!
//!   - capability:  Cp / Cpl / Cpu / Cpk / Cpm  (within σ, I-MR: MR̄/d2(2))
//!                  Pp / Ppl / Ppu / Ppk / Ppm / PT (overall σ = sample sd)
//!                  normal-approx nonconformance (pct_nc, ppm)
//!   - charts:      I-MR (individuals + moving range)
//!                  X̄-R (subgroups of size k) with Montgomery Table-VII
//!                  constants A2/D3/D4/d2.
//!
//! Conventions (JASP jaspQualityControl / Montgomery "Introduction to
//! Statistical Quality Control"):
//!   - within σ for individuals = MR̄ / d2(2), d2(2) = 1.128
//!   - overall σ = sample standard deviation (ddof = 1)
//!   - Cpk = min of the present one-sided values (cpu/cpl); Cpm = Bester/JMP
//!     target-based index (needs usl + lsl + target)
//!   - A2(k) = 3 / (d2(k) · √k), published table used for k = 2..10

use libm::{erfc, sqrt};

/// Standard SPC constants, subgroup size k = 2..10 (Montgomery Table VII).
pub const A2: [f64; 11] = [0.0, 0.0, 1.880, 1.023, 0.729, 0.577, 0.483, 0.419, 0.373, 0.337, 0.308];
pub const D3: [f64; 11] = [0.0, 0.0, 0.000, 0.000, 0.000, 0.000, 0.030, 0.118, 0.187, 0.242, 0.293];
pub const D4: [f64; 11] = [0.0, 0.0, 3.267, 2.574, 2.282, 2.114, 2.004, 1.924, 1.863, 1.815, 1.777];
pub const D2: [f64; 11] = [0.0, 0.0, 1.128, 1.693, 2.059, 2.326, 2.534, 2.704, 2.847, 2.970, 3.078];

/// Parse a flat numeric string ("20.1,19.8\n5.2 ...") into f64s.
/// Delimiters: whitespace and commas. Non-numeric tokens are an error so
/// typos are caught rather than silently skipped.
pub fn parse_numbers(s: &str) -> Result<Vec<f64>, ()> {
    let mut out: Vec<f64> = Vec::with_capacity(s.len() / 4);
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<f64>| -> Result<(), ()> {
        if !buf.is_empty() {
            buf.parse::<f64>().map(|v| out.push(v)).map_err(|_| ())?;
            buf.clear();
        }
        Ok(())
    };
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
            buf.push(c);
        } else if c.is_whitespace() || c == ',' {
            flush(&mut buf, &mut out)?;
        } else {
            return Err(());
        }
    }
    flush(&mut buf, &mut out)?;
    Ok(out)
}

/// Parse a spec-limit / target given as a string. Empty string, or the
/// tokens `NA` / `none` / `NULL` / `-` (case-insensitive) mean "absent".
/// (VARCHAR is used instead of DOUBLE-with-NULL because DuckDB auto-
/// propagates NULL inputs to the scalar output without invoking the
/// function, so NULL can't serve as the "absent" sentinel.)
pub fn parse_limit(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty()
        || t.eq_ignore_ascii_case("na")
        || t.eq_ignore_ascii_case("none")
        || t.eq_ignore_ascii_case("null")
        || t == "-"
    {
        return None;
    }
    t.parse::<f64>().ok()
}

/// Standard normal CDF Φ(x) via erfc.
pub fn norm_cdf(x: f64) -> f64 {
    0.5 * erfc(-x / sqrt(2.0))
}

/// Sample mean.
pub fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len() as f64
}

/// Sample standard deviation (ddof = 1, the "overall" process sd).
pub fn sd_sample(x: &[f64]) -> f64 {
    let m = mean(x);
    let ss: f64 = x.iter().map(|v| (v - m) * (v - m)).sum();
    if x.len() < 2 {
        return f64::NAN;
    }
    sqrt(ss / (x.len() - 1) as f64)
}

/// Mean absolute consecutive (successive) moving range.
pub fn mrbar(x: &[f64]) -> f64 {
    if x.len() < 2 {
        return f64::NAN;
    }
    let mut sum = 0.0;
    let mut cnt = 0usize;
    for w in x.windows(2) {
        sum += (w[1] - w[0]).abs();
        cnt += 1;
    }
    sum / cnt as f64
}

/// Within-process σ for individuals (I-MR): MR̄ / d2(2).
pub fn sd_within_imr(x: &[f64]) -> f64 {
    mrbar(x) / D2[2]
}

/// Process capability / performance indices.
///
/// `usl`, `lsl`, `target`: None = that limit/target is not supplied
/// (matching JASP's boundary flags). Returns a map of metric→Option<f64>.
pub fn capability(x: &[f64], usl: Option<f64>, lsl: Option<f64>, target: Option<f64>)
    -> std::collections::BTreeMap<String, Option<f64>>
{
    let mut out = std::collections::BTreeMap::new();
    let n = x.len() as f64;
    let m = mean(x);
    let s_o = sd_sample(x);            // overall
    let sw = sd_within_imr(x);         // within (I-MR)
    out.insert("n".into(), Some(x.len() as f64));
    out.insert("mean".into(), Some(m));
    out.insert("sd_overall".into(), Some(s_o));
    out.insert("sd_within".into(), Some(sw));
    out.insert("mrbar".into(), Some(mrbar(x)));

    // within
    let cpl = lsl.map(|l| (m - l) / (3.0 * sw));
    let cpu = usl.map(|u| (u - m) / (3.0 * sw));
    let cp = match (usl, lsl) {
        (Some(u), Some(l)) => Some((u - l) / (6.0 * sw)),
        _ => None,
    };
    let cpk = match (cpu, cpl) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let cpm = match (target, usl, lsl) {
        (Some(t), Some(u), Some(l)) => {
            let denom = sw * sqrt(1.0 + ((m - t) / sw) * ((m - t) / sw));
            if denom != 0.0 { Some((u - l) / (6.0 * denom)) } else { None }
        }
        _ => None,
    };
    out.insert("cp".into(), cp);
    out.insert("cpl".into(), cpl);
    out.insert("cpu".into(), cpu);
    out.insert("cpk".into(), cpk);
    out.insert("cpm".into(), cpm);

    // overall / performance
    let ppl = lsl.map(|l| (m - l) / (3.0 * s_o));
    let ppu = usl.map(|u| (u - m) / (3.0 * s_o));
    let pp = match (usl, lsl) {
        (Some(u), Some(l)) => Some((u - l) / (6.0 * s_o)),
        _ => None,
    };
    let ppk = match (ppu, ppl) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let ppm = match (target, usl, lsl) {
        (Some(t), Some(u), Some(l)) => {
            let denom = s_o * sqrt(1.0 + ((m - t) / s_o) * ((m - t) / s_o));
            if denom != 0.0 { Some((u - l) / (6.0 * denom)) } else { None }
        }
        _ => None,
    };
    let pt = pp; // process tolerance = usl−lsl over 6·overall sd
    out.insert("pp".into(), pp);
    out.insert("ppl".into(), ppl);
    out.insert("ppu".into(), ppu);
    out.insert("ppk".into(), ppk);
    out.insert("ppm".into(), ppm);
    out.insert("pt".into(), pt);

    // normal-approx nonconformance using overall sd
    let below = lsl.map(|l| norm_cdf((l - m) / s_o)).unwrap_or(0.0);
    let above = usl.map(|u| 1.0 - norm_cdf((u - m) / s_o)).unwrap_or(0.0);
    let frac = below + above;
    out.insert("pct_nc".into(), Some(frac * 100.0));
    out.insert("ppm".into(), Some(frac * 1e6));
    let _ = n;
    out
}

/// I-MR chart limits (individuals + moving range).
pub fn chart_i_mr(x: &[f64]) -> std::collections::BTreeMap<String, f64> {
    let mut out = std::collections::BTreeMap::new();
    let m = mean(x);
    let mb = mrbar(x);
    let sw = mb / D2[2];
    out.insert("cl".into(), m);
    out.insert("i_ucl".into(), m + 3.0 * sw);
    out.insert("i_lcl".into(), m - 3.0 * sw);
    out.insert("mr_cl".into(), mb);
    out.insert("mr_ucl".into(), D4[2] * mb);
    out.insert("mr_lcl".into(), D3[2] * mb);
    out
}

/// X̄-R chart limits for subgroups of size `k` (values are consecutive
/// k-groups, row-major).
pub fn chart_xbar_r(x: &[f64], k: usize) -> Option<std::collections::BTreeMap<String, f64>> {
    if k < 2 || k > 10 || x.is_empty() || x.len() % k != 0 {
        return None;
    }
    let g = x.len() / k;
    let mut sums = vec![0.0f64; g];
    let mut ranges = vec![0.0f64; g];
    for gi in 0..g {
        let seg = &x[gi * k..(gi + 1) * k];
        let s: f64 = seg.iter().sum();
        let mn = seg.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = seg.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        sums[gi] = s;
        ranges[gi] = mx - mn;
    }
    let xdbar = sums.iter().sum::<f64>() / ((g * k) as f64);
    let rbar = ranges.iter().sum::<f64>() / g as f64;
    let mut out = std::collections::BTreeMap::new();
    out.insert("xbar_cl".into(), xdbar);
    out.insert("xbar_ucl".into(), xdbar + A2[k] * rbar);
    out.insert("xbar_lcl".into(), xdbar - A2[k] * rbar);
    out.insert("r_cl".into(), rbar);
    out.insert("r_ucl".into(), D4[k] * rbar);
    out.insert("r_lcl".into(), D3[k] * rbar);
    Some(out)
}
