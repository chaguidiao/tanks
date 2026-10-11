//! Stable, platform-independent hashing (FNV-1a 64) for determinism checks and bake manifests.

#[derive(Debug, Clone)]
pub struct StableHasher(u64);

impl Default for StableHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl StableHasher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub fn u8(&mut self, v: u8) {
        self.write(&[v]);
    }
    pub fn u32(&mut self, v: u32) {
        self.write(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.write(&v.to_bits().to_le_bytes());
    }
    pub fn f64(&mut self, v: f64) {
        self.write(&v.to_bits().to_le_bytes());
    }
    pub fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.write(s.as_bytes());
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

pub fn hash_f32s(data: &[f32]) -> u64 {
    let mut h = StableHasher::new();
    for &v in data {
        h.f32(v);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_reference_values() {
        // Reference vectors from the FNV specification (Fowler/Noll/Vo, draft-eastlake-fnv).
        let mut h = StableHasher::new();
        assert_eq!(h.finish(), 0xcbf29ce484222325);
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63dc4c8601ec8c);
        let mut h = StableHasher::new();
        h.write(b"foobar");
        assert_eq!(h.finish(), 0x85944171f73967e8);
    }
}
