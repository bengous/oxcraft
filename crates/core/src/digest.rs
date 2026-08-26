//! Stable hashing for goldens: FNV-1a over bytes, bit-identical on every
//! platform, so pinned values compare across machines.

/// Incremental FNV-1a 64-bit hasher.
#[derive(Clone, Copy, Debug)]
pub struct Fnv1a(u64);

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv1a {
    /// The empty hash.
    #[must_use]
    pub const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    /// Absorbs raw bytes.
    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Absorbs the bit pattern of an `f32`, little-endian.
    pub fn write_f32(&mut self, value: f32) {
        self.write(&value.to_bits().to_le_bytes());
    }

    /// Absorbs an `i32`, little-endian.
    pub fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    /// The hash of everything absorbed so far.
    #[must_use]
    pub const fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_offset_basis() {
        assert_eq!(Fnv1a::new().finish(), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn known_vector_matches_reference_fnv1a() {
        let mut h = Fnv1a::new();
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn f32_and_i32_writes_are_byte_stable() {
        let mut a = Fnv1a::new();
        a.write_f32(1.5);
        a.write_i32(-7);
        let mut b = Fnv1a::new();
        b.write(&1.5f32.to_bits().to_le_bytes());
        b.write(&(-7i32).to_le_bytes());
        assert_eq!(a.finish(), b.finish());
    }
}
