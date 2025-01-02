/// Returns whether a bit at the given position is `1` or not.
pub(crate) fn is_bitflagged(byte: u8, position: u8) -> bool {
    let mask = 1 << (7 - position);
    byte & mask == mask
}
