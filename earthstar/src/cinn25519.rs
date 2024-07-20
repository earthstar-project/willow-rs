use arbitrary::{size_hint::and_all, Arbitrary, Error as ArbitraryError};
use either::Either;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};
use willow_data_model::encoding::{
    error::{DecodeError, EncodingConsumerError},
    parameters::{Decodable, Encodable},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Shortname<const MIN_LENGTH: usize, const MAX_LENGTH: usize>(pub Vec<u8>);

#[derive(Debug)]
pub enum ShortnameError {
    TooShort,
    TooLong,
    InvalidByte(usize),
    StartsWithNumber,
}

impl<const MIN_LENGTH: usize, const MAX_LENGTH: usize> Shortname<MIN_LENGTH, MAX_LENGTH> {
    pub fn new(shortname: &str) -> Result<Self, ShortnameError> {
        if shortname.len() < MIN_LENGTH {
            return Err(ShortnameError::TooShort);
        }

        if shortname.len() > MAX_LENGTH {
            return Err(ShortnameError::TooLong);
        }

        if shortname.as_bytes()[0].is_ascii_digit() {
            return Err(ShortnameError::StartsWithNumber);
        }

        for (i, byte) in shortname.as_bytes().iter().enumerate() {
            if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() {
                return Err(ShortnameError::InvalidByte(i));
            }
        }

        let mut vec = Vec::new();
        vec.extend_from_slice(shortname.as_bytes());

        Ok(Self(vec))
    }
}

impl<'a, const MIN_LENGTH: usize, const MAX_LENGTH: usize> Arbitrary<'a>
    for Shortname<MIN_LENGTH, MAX_LENGTH>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let mut len: usize = Arbitrary::arbitrary(u)?;
        len = MIN_LENGTH + (len % (1 + MAX_LENGTH - MIN_LENGTH));

        let mut s = String::new();

        if len == 0 {
            return Self::new("").map_err(|_| ArbitraryError::IncorrectFormat);
        } else {
            let mut alpha: u8 = Arbitrary::arbitrary(u)?;
            alpha %= 26; // There are 26 lowercase ascii alphabetic chars.
            alpha += 0x61; // 0x61 is ASCII 'a'.
            s.push(alpha as char);

            for _ in 1..len {
                let mut alphanum: u8 = Arbitrary::arbitrary(u)?;
                alphanum %= 36; // There are 36 lowercase ascii alphabetic chars or ascii digits.

                if alphanum < 26 {
                    alphanum += 0x61;
                } else {
                    alphanum = alphanum + 0x30 - 26; // It works, alright? Add the ascii code of '0', but subtract 26, because we transform numbers frmo 26 to 36, not from 0 to 10. (all those ranges with an inclusive start, exclusive end)
                }

                s.push(alphanum as char);
            }
        }

        Self::new(&s).map_err(|_| ArbitraryError::IncorrectFormat)
    }

    fn size_hint(_depth: usize) -> (usize, Option<usize>) {
        (MIN_LENGTH, Some(MAX_LENGTH))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cinn25519PublicKey<const MIN_LENGTH: usize, const MAX_LENGTH: usize> {
    pub shortname: Shortname<MIN_LENGTH, MAX_LENGTH>,
    pub underlying: [u8; 32],
}

impl<const MIN_LENGTH: usize, const MAX_LENGTH: usize> Cinn25519PublicKey<MIN_LENGTH, MAX_LENGTH> {
    // TODO: fn verify
}

impl<const MIN_LENGTH: usize, const MAX_LENGTH: usize> Encodable
    for Cinn25519PublicKey<MIN_LENGTH, MAX_LENGTH>
{
    async fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let mut vec = Vec::new();

        vec.extend_from_slice(&self.shortname.0);

        consumer.bulk_consume_full_slice(&self.shortname.0).await?;

        if MIN_LENGTH < MAX_LENGTH {
            consumer.consume(0x0).await?;
        }

        consumer.bulk_consume_full_slice(&self.underlying).await?;

        Ok(())
    }
}

impl<const MIN_LENGTH: usize, const MAX_LENGTH: usize> Decodable
    for Cinn25519PublicKey<MIN_LENGTH, MAX_LENGTH>
{
    async fn decode<Producer>(
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<<Producer>::Error>>
    where
        Producer: BulkProducer<Item = u8>,
    {
        if MIN_LENGTH == MAX_LENGTH {
            let mut shortname_box = Box::new_uninit_slice(MIN_LENGTH);

            let shortname_bytes = producer
                .bulk_overwrite_full_slice_uninit(shortname_box.as_mut())
                .await?;

            let mut underlying_slice = [0u8; 32];

            producer
                .bulk_overwrite_full_slice(&mut underlying_slice)
                .await?;

            return Ok(Self {
                shortname: Shortname(shortname_bytes.to_vec()),
                underlying: underlying_slice,
            });
        }

        let mut shortname_vec: Vec<u8> = Vec::new();

        loop {
            match producer.produce().await {
                Ok(Either::Left(item)) => {
                    // if item is 0x0, stop
                    if item == 0x0 {
                        break;
                    }

                    shortname_vec.push(item);
                }
                Ok(Either::Right(_)) => {
                    // whatever happens, we won't be able to make a full public key.
                    // Error!
                    return Err(DecodeError::InvalidInput);
                }
                Err(err) => return Err(DecodeError::Producer(err)),
            }
        }

        let mut underlying_slice = [0u8; 32];

        producer
            .bulk_overwrite_full_slice(&mut underlying_slice)
            .await?;

        Ok(Self {
            shortname: Shortname(shortname_vec),
            underlying: underlying_slice,
        })
    }
}

impl<'a, const MIN_LENGTH: usize, const MAX_LENGTH: usize> Arbitrary<'a>
    for Cinn25519PublicKey<MIN_LENGTH, MAX_LENGTH>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let shortname: Shortname<MIN_LENGTH, MAX_LENGTH> = Arbitrary::arbitrary(u)?;
        let underlying: [u8; 32] = Arbitrary::arbitrary(u)?;

        Ok(Self {
            shortname,
            underlying,
        })
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        and_all(&[
            Shortname::<MIN_LENGTH, MAX_LENGTH>::size_hint(depth),
            (32, Some(32)),
        ])
    }
}

pub struct Cinn25519SecretKey<const MIN_LENGTH: usize, const MAX_LENGTH: usize> {
    pub shortname: Shortname<MIN_LENGTH, MAX_LENGTH>,
    pub underlying: [u8; 32],
}

impl<const MIN_LENGTH: usize, const MAX_LENGTH: usize> Cinn25519SecretKey<MIN_LENGTH, MAX_LENGTH> {
    // TODO: fn sign(&self, message) -> Result
}

pub struct Cinn25519Keypair<const MIN_LENGTH: usize, const MAX_LENGTH: usize> {
    pub public_key: Cinn25519PublicKey<MIN_LENGTH, MAX_LENGTH>,
    pub secret_key: Cinn25519SecretKey<MIN_LENGTH, MAX_LENGTH>,
}

impl<const MIN_LENGTH: usize, const MAX_LENGTH: usize> Cinn25519Keypair<MIN_LENGTH, MAX_LENGTH> {
    // TODO: fn new(public_key, secret_key) -> Result
}
