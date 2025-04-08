//! Utilities for implementing Willow's various [encodings](https://willowprotocol.org/specs/encodings/index.html#encodings).

/// Returns whether a bit at the given position is `1` or not. Position `0` is the most significant bit, position `7` the least significant bit.
pub fn is_bitflagged(byte: u8, position: u8) -> bool {
    let mask = 1 << (7 - position);
    byte & mask == mask
}
