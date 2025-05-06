use ufotofu_codec::{DecodableCanonic, DecodableSync};

/// The Diffie-Hellman-related parameters. The type implementing this is the type of secret keys.
pub trait DiffieHellmanSecretKey {
    /// A *valid* public key that can be used for the Diffie-Hellman exchange without any problems. For most schemes, this is **not** simply a byte array of a certain length (for example, Curve25519 is insecure when one party inputs an all-zeroes public key). It is the job of the [`ufotofu_codec::Decodable`] implementation to reject such invalid keys.
    type PublicKey: DecodableCanonic + DecodableSync;

    /// Must commute in the sense that `sk1.dh(pk2) == sk2.dh(pk1)`.
    fn dh(&self, dh: &Self::PublicKey) -> Self::PublicKey;
}

/// The AEAD-related parameters. The type implementing this is the type of encryption keys.
pub trait AEADEncryptionKey<
    const TAG_WIDTH_IN_BYTES: usize,
    const NONCE_LENGTH_IN_BYTES: usize,
    const IS_TAG_PREPENDED: bool,
>
{
    /// If IS_TAG_PREPENDED, the bytes starting at TAG_WIDTH_IN_BYTES are the plaintext, otherwise, all but the final TAG_WIDTH_IN_BYTES bytes are the plaintext. plaintext_with_additional_space must have a length of at least TAG_WIDTH_IN_BYTES.
    fn encrypt_inplace(
        &self,
        nonce: &[u8; NONCE_LENGTH_IN_BYTES],
        ad: &[u8],
        plaintext_with_additional_space: &mut [u8],
    );

    /// If IS_TAG_PREPENDED, the bytes starting at TAG_WIDTH_IN_BYTES will hold the plaintext, otherwise, all but the final TAG_WIDTH_IN_BYTES bytes will hold the plaintext. cyphertext_with_additional_space must have a length of at least TAG_WIDTH_IN_BYTES.
    fn decrypt_inplace(
        &self,
        nonce: &[u8; NONCE_LENGTH_IN_BYTES],
        ad: &[u8],
        cyphertext_with_additional_space: &mut [u8],
    ) -> Result<(), ()>;
}

/// The hashing-related parameters.
pub trait Hashing<const HASHLEN_IN_BYTES: usize, const BLOCKLEN_IN_BYTES: usize, AEADKey> {
    fn hash(data: &[u8]) -> [u8; HASHLEN_IN_BYTES];

    fn digest_to_aeadkey(digest: &[u8; HASHLEN_IN_BYTES]) -> AEADKey;
}
