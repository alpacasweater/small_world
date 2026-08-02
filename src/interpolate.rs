/// Catmull-Rom cubic interpolation between `values[1]` and `values[2]` at
/// parameter `t` in `[0, 1]` (clamped), using `values[0]` and `values[3]` as
/// outer support points.
pub fn cubic_unit(t: f64, values: [f64; 4]) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;

    let p0 = values[0];
    let p1 = values[1];
    let p2 = values[2];
    let p3 = values[3];

    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Separable bicubic interpolation over a 4x4 `grid` at fractional offsets
/// `(tx, ty)` within the central cell: [`cubic_unit`] along each row in `tx`,
/// then once across the row results in `ty`.
pub fn bicubic_unit(tx: f64, ty: f64, grid: [[f64; 4]; 4]) -> f64 {
    let mut rows = [0.0_f64; 4];
    for row in 0..4 {
        rows[row] = cubic_unit(tx, grid[row]);
    }
    cubic_unit(ty, rows)
}

#[cfg(test)]
mod tests {
    use super::{bicubic_unit, cubic_unit};

    #[test]
    fn cubic_respects_endpoints() {
        let values = [0.0, 1.0, 2.0, 3.0];
        assert!((cubic_unit(0.0, values) - 1.0).abs() < 1e-12);
        assert!((cubic_unit(1.0, values) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn bicubic_constant_surface() {
        let value = 42.5;
        let grid = [[value; 4]; 4];
        let result = bicubic_unit(0.3, 0.7, grid);
        assert!((result - value).abs() < 1e-12);
    }
}
