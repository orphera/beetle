//! The 8x8 inverse DCT, separable, in floating point (the IEEE 1180
//! reference form), rounded to integers. Decoders differ here by ±1 now
//! and then; the standard allows it.

use std::sync::OnceLock;

/// `c(u) * cos((2x + 1) u π / 16) / 2` for row x, column u.
fn basis() -> &'static [[f32; 8]; 8] {
    static TABLE: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [[0f32; 8]; 8];
        for (x, row) in t.iter_mut().enumerate() {
            for (u, v) in row.iter_mut().enumerate() {
                let c = if u == 0 {
                    std::f64::consts::FRAC_1_SQRT_2
                } else {
                    1.0
                };
                *v = (c * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0).cos() / 2.0)
                    as f32;
            }
        }
        t
    })
}

/// Transforms `block` (coefficients, row-major) in place into samples.
pub fn idct(block: &mut [i32; 64]) {
    let b = basis();
    let mut tmp = [0f32; 64];
    // Rows: for each coefficient row v, samples along x.
    for v in 0..8 {
        let row = &block[v * 8..v * 8 + 8];
        if row.iter().all(|&c| c == 0) {
            continue;
        }
        for x in 0..8 {
            let mut s = 0f32;
            for u in 0..8 {
                s += b[x][u] * row[u] as f32;
            }
            tmp[v * 8 + x] = s;
        }
    }
    // Columns.
    for x in 0..8 {
        for y in 0..8 {
            let mut s = 0f32;
            for v in 0..8 {
                s += b[y][v] * tmp[v * 8 + x];
            }
            block[y * 8 + x] = s.round() as i32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dc_coefficient_is_a_flat_block() {
        let mut block = [0i32; 64];
        block[0] = 80; // DC 80 → 80 / 8 = 10 everywhere
        idct(&mut block);
        assert!(block.iter().all(|&v| v == 10), "{block:?}");
    }

    #[test]
    fn zeros_stay_zero() {
        let mut block = [0i32; 64];
        idct(&mut block);
        assert!(block.iter().all(|&v| v == 0));
    }
}
