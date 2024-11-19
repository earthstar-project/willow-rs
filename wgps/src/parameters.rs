pub trait ChallengeHash<const CHALLENGE_LENGTH: usize, const CHALLENGE_HASH_LENGTH: usize> {
    fn hash(nonce: [u8; CHALLENGE_LENGTH]) -> [u8; CHALLENGE_HASH_LENGTH];
}
