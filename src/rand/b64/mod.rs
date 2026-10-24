use crate::{Rng, cbrng::b64::*, prng::b64::*};

crate::impl_rand_trait64! {
    Biski64,
    Cet64,
    Cet256,
    Mt1993764,
    Sfmt1993764,
    Philox64,
    Sfc64,
    SplitMix64,
    Threefish256,
    TwistedGFSR,
    Xoroshiro128Pp,
    Xoroshiro128Ss,
    Xorshift64,
    Xoshiro256Pp,
    Xoshiro256Ss,
    WyHash64
}

crate::impl_try_rng_trait64! {
    Biski64,
    Cet64,
    Cet256,
    Mt1993764,
    Sfmt1993764,
    Philox64,
    Sfc64,
    SplitMix64,
    Threefish256,
    TwistedGFSR,
    Xoroshiro128Pp,
    Xoroshiro128Ss,
    Xorshift64,
    Xoshiro256Pp,
    Xoshiro256Ss,
    WyHash64
}

#[cfg(test)]
mod tests {
    use rand::{Rng, SeedableRng};
    use crate::Rng as UrngRng;

    use super::*;

    #[test]
    fn seed_bridge_is_little_endian_and_full_width() {
        // Issue A6: the 64-bit generators expose the full u64 seed word as
        // [u8; 8] in little-endian byte order (the old bridge used
        // [u8; 4] + native-endian, truncating the space to 2^32 and
        // diverging on big-endian hosts).
        let mut seeded = Sfc64::from_seed(9u64.to_le_bytes());
        let mut direct = Sfc64::new(9);
        assert_eq!(seeded.next_u64(), direct.nextu());

        // Nonzero bytes must actually reach the seed word (all 8 used).
        let mut seeded = Xoshiro256Ss::from_seed([0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        let mut direct = Xoshiro256Ss::new(0x8877_6655_4433_2211);
        assert_eq!(seeded.next_u64(), direct.nextu());

        let mut seeded = Sfc64::from_seed([0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        let mut direct = Sfc64::new(0x8877_6655_4433_2211);
        assert_eq!(seeded.next_u64(), direct.nextu());
    }
}
