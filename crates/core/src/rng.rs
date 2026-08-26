//! Linear congruential generator shared by every procedural painter and
//! spawner; the fixed constants keep every consumer bit-identical across
//! platforms.

/// Deterministic 32-bit LCG state.
#[derive(Clone, Copy, Debug)]
pub struct Lcg(u32);

impl Lcg {
    /// A generator starting from `seed`.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self(seed)
    }

    /// Next value in `[0, 1]`.
    pub fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 as f32 / u32::MAX as f32
    }

    /// Next value in `[lo, hi]`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcg_is_deterministic_and_in_unit_range() {
        let mut a = Lcg::new(7);
        let mut b = Lcg::new(7);
        for _ in 0..1000 {
            let x = a.next_f32();
            assert!((0.0..=1.0).contains(&x));
            assert_eq!(x.to_bits(), b.next_f32().to_bits());
        }
        let mut c = Lcg::new(7);
        let r = c.range(2.0, 4.0);
        assert!((2.0..=4.0).contains(&r));
    }
}
