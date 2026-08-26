//! Integer-hash value noise and fractal helpers.
//!
//! All functions are pure and seeded: identical inputs always produce
//! bit-identical outputs, which the generation tests rely on.

/// Hashes `(x, y)` into a pseudo-random float in `[0, 1)`.
///
/// FIXME: numerators in `0xFFFFFF80..u32::MAX` round to `2^32` in the f32
/// cast and `u32::MAX as f32` also rounds to `2^32`, so those inputs return
/// exactly `1.0` and can push tree height past `tree_max_h` (hash3 shares
/// the pattern). Fixing this changes generated worlds and is deliberately
/// not done in the tuning-naming branch.
pub fn hash2(x: i32, y: i32, seed: i32) -> f32 {
    let mut h = seed ^ x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
    h = (h as u32 ^ (h as u32 >> 13)).wrapping_mul(1_274_126_177) as i32;
    ((h as u32) ^ ((h as u32) >> 16)) as f32 / u32::MAX as f32
}

/// Hashes `(x, y, z)` into a pseudo-random float in `[0, 1)`.
pub fn hash3(x: i32, y: i32, z: i32, seed: i32) -> f32 {
    let mut h = seed
        ^ x.wrapping_mul(374_761_393)
        ^ y.wrapping_mul(668_265_263)
        ^ z.wrapping_mul(2_246_822_519u32 as i32);
    h = (h as u32 ^ (h as u32 >> 13)).wrapping_mul(1_274_126_177) as i32;
    ((h as u32) ^ ((h as u32) >> 16)) as f32 / u32::MAX as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// 2D value noise with smooth interpolation, in `[0, 1]`.
pub fn value_noise2(x: f32, y: f32, seed: i32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - xi as f32;
    let yf = y - yi as f32;
    let v00 = hash2(xi, yi, seed);
    let v10 = hash2(xi + 1, yi, seed);
    let v01 = hash2(xi, yi + 1, seed);
    let v11 = hash2(xi + 1, yi + 1, seed);
    let u = smooth(xf);
    let v = smooth(yf);
    v00 * (1.0 - u) * (1.0 - v) + v10 * u * (1.0 - v) + v01 * (1.0 - u) * v + v11 * u * v
}

/// 3D value noise with smooth interpolation, in `[0, 1]`.
pub fn value_noise3(x: f32, y: f32, z: f32, seed: i32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let zi = z.floor() as i32;
    let xf = x - xi as f32;
    let yf = y - yi as f32;
    let zf = z - zi as f32;
    let u = smooth(xf);
    let v = smooth(yf);
    let w = smooth(zf);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let n00 = lerp(hash3(xi, yi, zi, seed), hash3(xi + 1, yi, zi, seed), u);
    let n10 = lerp(
        hash3(xi, yi + 1, zi, seed),
        hash3(xi + 1, yi + 1, zi, seed),
        u,
    );
    let n01 = lerp(
        hash3(xi, yi, zi + 1, seed),
        hash3(xi + 1, yi, zi + 1, seed),
        u,
    );
    let n11 = lerp(
        hash3(xi, yi + 1, zi + 1, seed),
        hash3(xi + 1, yi + 1, zi + 1, seed),
        u,
    );
    lerp(lerp(n00, n10, v), lerp(n01, n11, v), w)
}

/// Fractal Brownian motion over [`value_noise2`], normalized to `[0, 1]`.
///
/// Each octave doubles the frequency and halves the amplitude; `octaves`
/// clamps the detail level.
pub fn fbm2(x: f32, y: f32, seed: i32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    let (mut nx, mut ny) = (x, y);
    for i in 0..octaves {
        let oi = (i as i32).wrapping_mul(1013);
        sum += value_noise2(nx, ny, seed.wrapping_add(oi)) * amp;
        norm += amp;
        nx *= 2.0;
        ny *= 2.0;
        amp *= 0.5;
    }
    sum / norm
}

/// Fractal Brownian motion over [`value_noise3`], normalized to `[0, 1]`.
pub fn fbm3(x: f32, y: f32, z: f32, seed: i32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    let (mut nx, mut ny, mut nz) = (x, y, z);
    for i in 0..octaves {
        let oi = (i as i32).wrapping_mul(1013);
        sum += value_noise3(nx, ny, nz, seed.wrapping_add(oi)) * amp;
        norm += amp;
        nx *= 2.0;
        ny *= 2.0;
        nz *= 2.0;
        amp *= 0.5;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::float_cmp,
        reason = "hashing is integer math; exact equality pins determinism regressions"
    )]
    use super::*;

    #[test]
    fn hash2_is_deterministic_and_in_unit_range() {
        for i in -50..50 {
            for j in -50..50 {
                let a = hash2(i, j, 1337);
                let b = hash2(i, j, 1337);
                assert_eq!(a, b);
                assert!((0.0..=1.0).contains(&a));
            }
        }
    }

    #[test]
    fn fbm3_is_deterministic() {
        let a = fbm3(1.7, 2.3, 3.9, 42, 2);
        let b = fbm3(1.7, 2.3, 3.9, 42, 2);
        assert_eq!(a, b);
        assert!((0.0..=1.0).contains(&a));
    }
}
