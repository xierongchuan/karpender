pub(super) fn initial_jitter_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0x9e37_79b9)
        .max(1)
}

pub(super) fn xorshift32(mut value: u32) -> u32 {
    if value == 0 {
        value = 0x9e37_79b9;
    }
    value ^= value << 13;
    value ^= value >> 17;
    value ^= value << 5;
    value
}
