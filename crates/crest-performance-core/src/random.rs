//! Stateless SplitMix64 words with explicit wrapping arithmetic. Callers own
//! seed/identity selection and version their salt meanings in provenance.
pub(crate) fn word(seed: u64, step: u64, lane: u64) -> u64 {
    let mut value = seed
        .wrapping_add(step.wrapping_mul(0x9e3779b97f4a7c15))
        .wrapping_add(lane.wrapping_mul(0xd1b54a32d192ed03));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}
