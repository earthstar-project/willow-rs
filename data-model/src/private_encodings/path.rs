#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use compact_u64::CompactU64;
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, RelativeDecodable, RelativeEncodable,
};

use crate::{decode_path_extends_path, encode_path_extends_path, Path};

#[derive(Debug)]
/// The context necessary to privately encode Paths.
pub struct PrivatePathContext<const MCL: usize, const MCC: usize, const MPL: usize> {
    /// The Path whose Components are to be kept private.
    private: Path<MCL, MCC, MPL>,
    /// The prefix relative to which we encode.
    rel: Path<MCL, MCC, MPL>,
}

#[derive(Debug)]
pub struct ComponentsNotRelatedError {}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PrivatePathContext<MCL, MCC, MPL> {
    pub fn new(
        private: Path<MCL, MCC, MPL>,
        rel: Path<MCL, MCC, MPL>,
    ) -> Result<Self, ComponentsNotRelatedError> {
        if !private.is_related(&rel) {
            return Err(ComponentsNotRelatedError {});
        }

        Ok(Self { private, rel })
    }

    pub fn new_unchecked(private: Path<MCL, MCC, MPL>, rel: Path<MCL, MCC, MPL>) -> Self {
        Self { private, rel }
    }

    pub fn private(&self) -> &Path<MCL, MCC, MPL> {
        &self.private
    }

    pub fn rel(&self) -> &Path<MCL, MCC, MPL> {
        &self.rel
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
    for PrivatePathContext<MCL, MCC, MPL>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let private: Path<MCL, MCC, MPL> = Arbitrary::arbitrary(u)?;
        let rel: Path<MCL, MCC, MPL> = Arbitrary::arbitrary(u)?;

        Ok(Self { private, rel })
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeEncodable<PrivatePathContext<MCL, MCC, MPL>> for Path<MCL, MCC, MPL>
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &PrivatePathContext<MCL, MCC, MPL>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        if !r.rel.is_prefix_of(self) {
            panic!("Tried to encode a path relative to a PrivatePathContext.rel path it is not prefixed by")
        }

        if !self.is_related(&r.private) {
            panic!("Tried to encode a path relative to a PrivatePathContext.private pat it is not related to")
        }

        let rel_count = r.rel.component_count();
        let private_count = r.private.component_count();

        if private_count <= rel_count {
            // path extends path val <> rel
            encode_path_extends_path(consumer, self, &r.rel).await?;
        } else {
            let lcp = self.longest_common_prefix(&r.private);

            let lcp_len = lcp.component_count();
            CompactU64(lcp.component_count() as u64)
                .encode(consumer)
                .await?;

            if lcp_len >= private_count {
                // path extends path val <> priv
                encode_path_extends_path(consumer, self, r.private()).await?;
            }
        }

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeDecodable<PrivatePathContext<MCL, MCC, MPL>, Blame> for Path<MCL, MCC, MPL>
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &PrivatePathContext<MCL, MCC, MPL>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let rel_count = r.rel.component_count();
        let private_count = r.private.component_count();

        if private_count <= rel_count {
            decode_path_extends_path(producer, r.rel()).await
        } else {
            // Decode C64 of length of longest common prefix of priv with val
            let private_component_count = CompactU64::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?;

            if private_component_count.0 >= private_count as u64 {
                decode_path_extends_path(producer, r.private()).await
            } else {
                // We can unwrap here because we know private_component_count will be less than the component count of r.private
                r.private
                    .create_prefix(private_component_count.0 as usize)
                    .ok_or(DecodeError::Other(Blame::TheirFault))
            }
        }
    }
}
