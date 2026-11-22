// SPDX-License-Identifier: Apache-2.0 AND MIT

//! Significance helpers. Two arms, binary outcome per cell, so the honest test
//! is Fisher's exact on the 2x2 table: no normal approximation, which matters
//! at the cell counts these suites run. A difference this suite cannot
//! separate from noise is reported as such. `benchmarks/stats.js` keeps the
//! same functions for `benchmarks/quality/grade.js`, which stays in
//! JavaScript because it runs the models' code.

use crate::js;

/// ln Γ(x), Lanczos.
fn lgamma(x: f64) -> f64 {
    const G: [f64; 6] = [
        76.180_091_729_471_46,
        -86.505_320_329_416_77,
        24.014_098_240_830_91,
        -1.231_739_572_450_155,
        0.120_865_097_386_617_9e-2,
        -0.539_523_938_495_3e-5,
    ];
    let mut y = x;
    let mut tmp = x + 5.5;
    tmp -= (x + 0.5) * tmp.ln();
    let mut ser = 1.000_000_000_190_015;
    for g in G {
        y += 1.0;
        ser += g / y;
    }
    -tmp + (2.506_628_274_631_000_5 * ser / x).ln()
}

fn lchoose(n: f64, k: f64) -> f64 {
    lgamma(n + 1.0) - lgamma(k + 1.0) - lgamma(n - k + 1.0)
}

/// Two-sided Fisher exact p for `[[a, b], [c, d]]`.
#[must_use]
pub fn fisher(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let n = a + b + c + d;
    let lp = |x: f64| lchoose(a + b, x) + lchoose(c + d, a + c - x) - lchoose(n, a + c);
    let obs = lp(a);
    let lo = f64::max(0.0, a + c - (c + d));
    let hi = f64::min(a + b, a + c);
    let mut p = 0.0;
    let mut x = lo;
    while x <= hi {
        let v = lp(x);
        if v <= obs + 1e-9 {
            p += v.exp();
        }
        x += 1.0;
    }
    f64::min(1.0, p)
}

/// Wilson score interval at 95%: honest at small n, unlike the textbook normal one.
#[must_use]
pub fn wilson(k: f64, n: f64) -> [f64; 2] {
    if n == 0.0 {
        return [0.0, 0.0];
    }
    let z = 1.96;
    let p = k / n;
    let z2 = z * z;
    let den = 1.0 + z2 / n;
    let c = (p + z2 / (2.0 * n)) / den;
    let half = (z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt()) / den;
    [f64::max(0.0, c - half), f64::min(1.0, c + half)]
}

#[must_use]
pub fn fmt_p(p: f64) -> String {
    if p < 0.001 {
        "<0.001".into()
    } else {
        js::to_fixed(p, 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_samples() {
        assert_eq!(wilson(0.0, 0.0), [0.0, 0.0]);
        assert_eq!(fmt_p(fisher(0.0, 1.0, 1.0, 0.0)), "1.000");
        assert_eq!(fmt_p(0.0004), "<0.001");
        assert_eq!(fmt_p(0.0512), "0.051");
    }
}
