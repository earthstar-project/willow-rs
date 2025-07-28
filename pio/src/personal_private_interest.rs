use crate::PrivateInterest;
#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use willow_data_model::{NamespaceId, SubspaceId};

#[derive(Debug)]
pub struct PersonalPrivateInterest<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    UserPublicKey: SubspaceId,
> {
    pub private_interest: PrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    pub user_key: UserPublicKey,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId,
        UserPublicKey: SubspaceId,
    > PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>
{
    pub fn private_interest(&self) -> &PrivateInterest<MCL, MCC, MPL, N, UserPublicKey> {
        &self.private_interest
    }

    pub fn user_key(&self) -> &UserPublicKey {
        &self.user_key
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, N: NamespaceId, S: SubspaceId>
    Arbitrary<'a> for PersonalPrivateInterest<MCL, MCC, MPL, N, S>
where
    N: NamespaceId + Arbitrary<'a>,
    S: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            private_interest: Arbitrary::arbitrary(u)?,
            user_key: Arbitrary::arbitrary(u)?,
        })
    }
}
