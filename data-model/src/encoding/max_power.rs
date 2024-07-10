/// Return the least natural number such that 256^`n` is greater than or equal to `n`.
///
/// Used for determining the minimal number of bytes needed to represent a given unsigned integer, and more specifically [`path_length_power`](https://willowprotocol.org/specs/encodings/index.html#path_length_power) and [`path_count_power`](https://willowprotocol.org/specs/encodings/index.html#path_count_power).
pub const fn max_power(n: usize) -> u8 {
    if n < 256 {
        1
    } else if n < 65536 {
        2
    } else if n < 16777216 {
        3
    } else if n < 4294967296 {
        4
    } else if n < (256_usize).pow(5) {
        5
    } else if n < (256_usize).pow(6) {
        6
    } else if n < (256_usize).pow(7) {
        7
    } else {
        8
    }
}
