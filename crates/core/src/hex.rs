/// Stable lowercase encoding for persisted hashes and API signatures.
/// Encode bytes directly instead of relying on a digest wrapper's fmt traits.
pub(crate) fn lower(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|&byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 0x0f)]),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::lower;
    use sha2::{Digest, Sha256};

    #[test]
    fn encoding_preserves_leading_zeroes_and_lowercase_digits() {
        assert_eq!(lower(&[]), "");
        assert_eq!(lower(&[0, 1, 15, 16, 128, 171, 255]), "00010f1080abff");
    }

    #[test]
    fn sha256_output_matches_existing_standard_vectors() {
        for (input, expected) in [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            assert_eq!(lower(&Sha256::digest(input.as_bytes())), expected);
        }
    }
}
