# Willow Test Vectors

This repository contains test vectors for Willow, primarily for its [encodings](https://willowprotocol.org/specs/encodings). All data sets use Willow25 parameters.

The different kinds of test data sets:

# Encodings

For each encoding, there is a directory with its name (e.g., `encode_path`, or `EncodeEntryRelativeEntry`). These fall into four categories: encoding relations, encoding functions, relative encoding relations, and relative encoding functions.

# Encoding Relations

For (absolute) encoding relations, e.g. `EncodePath`, the directory for the relation contains the following subdirectories:

- `yay`: Contains numbered files which consist of bytes that must be decoded successfully.
- `reencoded`: For each file in `yay`, there is a file of equal name in `reencoded` which contains the bytes obtained by reencoding the decoded value. The exact encoding varies: if there is a corresponding encoding *function*, we use it. Otherwise, you mihgt have to reconstruct the precise encoding we use, but generally: we use the shortest possible encoding, for arbitrary choices that do not impact the length we use the first one that occurs in the specification of the encoding relation, and for arbitrary bits we prefer zero bits.
- `dbg`: For each file in `yay`, there is a file of equal name in `dbg` which contains a human-readable representation of the decoded value.
- `nay`: Contains numbered files which consist of bytes that must *not* decode successfully.
- `nay_reason`: For each file in `nay`, there is a file of equal name in `nay_reason` which contains a human-readable representation of why decoding failed. These are typically rather coarse-grained, with the most common ones being "unexpected end of file" and "their fault" (the latter meaning that the code is invalid in *some* form).

# Encoding Functions

For (absolute) encoding functions, e.g. `encode_path`, the directory for the function is structured exactly as for an encoding relation, with one difference: there is no `reencoded` directory, since its contents would be identical to the `yay` directory. When using the test vectors, you should probably still reencode the value and verify that the result is equal to the input code.

# Relative Encoding Relations

For relative encoding relations, e.g. `EncodePathRelativePath`, the directory for the relation contains the following subdirectories:

- `yay`: Contains numbered files which consist of bytes that must be decoded successfully relative to the corresponding value encoded in `yay_relative_to`.
- `yay_relative_to`: For each file in `yay`, there is a file of equal name in `yay_relative_to` which contains an encoding of the value relative to which the bytes in `yay` must be decoded. The encoding of the relative value is always the absolute encoding (canonic whenever a canonic encoding is defined) for that kind of value defined in the Willow encodings spec. For tuples (primarily pairs), the encoding is simply the concatenation of the encodings of the successive tuple components. For types for which there is no absolute encoding defined in the willow specs, we use the encoding defined at the bottom of this file in the `Custom Encodings` section.
- `dbg`: For each file in `yay`, there is a file of equal name in `dbg` which contains a human-readable representation of the decoded value and the value relative to which it was decoded.
- `reencoded`: For each file in `yay`, there is a file of equal name in `reencoded` which contains the bytes obtained by reencoding the decoded value (relative to the value relative to which it was decoded). The exact encoding varies: if there is a corresponding encoding *function*, we use it. Otherwise, you mihgt have to reconstruct the precise encoding we use, but generally: we use the shortest possible encoding, for arbitrary choices that do not impact the length we use the first one that occurs in the specification of the encoding relation, and for arbitrary bits we prefer zero bits.
- `nay`: Contains numbered files which consist of bytes that must *not* decode successfully relative to the corresponding value encoded in `nay_relative_to`.
- `nay_relative_to`: For each file in `nay`, there is a file of equal name in `nay_relative_to` which contains an encoding of the value relative to which the bytes in `yay` must be decoded. The encoding of the relative value is always the absolute encoding (canonic whenever a canonic encoding is defined) for that kind of value defined in the Willow encodings spec.
- `nay_dbg`: For each file in `nay`, there is a file of equal name in `nay_dbg` which contains a human-readable representation of the value relative to which encoding must have been performed (and must have failed).
- `nay_reason`: For each file in `nay`, there is a file of equal name in `nay_reason` which contains a human-readable representation of why decoding failed. These are typically rather coarse-grained, with the most common ones being "unexpected end of file" and "their fault" (the latter meaning that the code is invalid in *some* form).

## Relative Encoding Functions

For relative encoding functions, e.g. `path_rel_path`, the directory for the funciton is structured the same way as for relative encoding *relations*.

# Gotchas

Test vectors for encoding and decoding capabilities ignore whether the signatures contained in those capabilities are correct.

# Custom Encodings

Encodings we need for test vector information but which are not defined in the willow spec.

## An Absolute Encoding Relation for 3dRange

- First byte is a header of bitflags:
    - most significant bit: `1` iff the subspace range is open.
    - second-most significant bit: `1` iff the path range is open.
    - third-most significant bit: `1` iff the timestamp range is open.
    - remaining five bits: arbitrary.
- encoding of the start of the subspace range
- encoding of the end of the subspace range, or empty string if the subspace range is open
- encoding of the start of the path range
- encoding of the end of the path range, or empty string if the path range is open
- encoding of the start of the timestamp range as an 8-byte big-endian integer
- encoding of the end of the timestamp range as an 8-byte big-endian integer, or empty string if the timestamp range is open

## An Absolute Encoding Relation for Area

- First byte is a header of bitflags:
    - most significant bit: `1` iff the subspace is `any`.
    - third-most significant bit: `1` iff the timestamp range is open.
    - remaining six bits: arbitrary.
- encoding of the subspace id, or empty string if the subspace is `any`
- encoding of the path
- encoding of the start of the timestamp range as an 8-byte big-endian integer
- encoding of the end of the timestamp range as an 8-byte big-endian integer, or empty string if the timestamp range is open
