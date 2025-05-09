use either::Either::{self, Left, Right};
use ufotofu::{
    BufferedConsumer, BufferedProducer, BulkConsumer, BulkProducer, ConsumeAtLeastError, Consumer,
    Producer,
};

use crate::parameters::AEADEncryptionKey;

/// The possible errors emitted by an encryptor.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EncryptionError<ConsumerError> {
    /// The inner consumer emitted an error.
    Inner(ConsumerError),
    /// Exhausted all possible nonces (happens after sending 2^64 messages).
    NoncesExhausted,
}

impl<ConsumerError: core::fmt::Display> core::fmt::Display for EncryptionError<ConsumerError> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionError::Inner(err) => err.fmt(f),
            EncryptionError::NoncesExhausted => {
                write!(
                    f,
                    "Exhausted all possible nonces (happens after sending 2^64 messages)."
                )
            }
        }
    }
}

impl<ConsumerError: std::error::Error> std::error::Error for EncryptionError<ConsumerError> {}

impl<ConsumerError> From<ConsumerError> for EncryptionError<ConsumerError> {
    fn from(value: ConsumerError) -> Self {
        EncryptionError::Inner(value)
    }
}

pub struct Encryptor<
    const TAG_WIDTH_IN_BYTES: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
    const NONCE_WIDTH_IN_BYTES: usize,
    const IS_TAG_PREPENDED: bool,
    AEADKey,
    C,
> {
    key: AEADKey,
    nonce: [u8; NONCE_WIDTH_IN_BYTES],
    buf: [u8; TAG_WIDTH_IN_BYTES_PLUS_4096],
    buffered_count: u16,
    c: C,
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        C,
    >
    Encryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        C,
    >
{
    pub fn new(key: AEADKey, inner: C) -> Self {
        Self {
            key,
            nonce: [0; NONCE_WIDTH_IN_BYTES],
            buf: [0; TAG_WIDTH_IN_BYTES_PLUS_4096],
            buffered_count: 0,
            c: inner,
        }
    }

    pub fn into_inner(self) -> C {
        self.c
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        C,
    >
    Encryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        C,
    >
where
    C: BulkConsumer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    async fn send_header(&mut self, len: u16) -> Result<(), EncryptionError<C::Error>> {
        let mut buf = [0; TAG_WIDTH_IN_BYTES_PLUS_2];
        let offset = if IS_TAG_PREPENDED {
            TAG_WIDTH_IN_BYTES
        } else {
            0
        };
        buf[offset] = len.to_be_bytes()[0];
        buf[offset + 1] = len.to_be_bytes()[1];

        self.key.encrypt_inplace(&self.nonce, &[], &mut buf[..]);
        increment_nonce(&mut self.nonce).map_err(|()| EncryptionError::NoncesExhausted)?;

        self.c
            .bulk_consume_full_slice(&buf[..])
            .await
            .map_err(ConsumeAtLeastError::into_reason)?;

        // println!("sent header {:?} (len {:?})", &buf[..], len);

        Ok(())
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        C,
    > Consumer
    for Encryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        C,
    >
where
    C: BulkConsumer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    type Item = u8;

    type Final = C::Final;

    type Error = EncryptionError<C::Error>;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        if self.buffered_count == 4096 {
            self.flush().await?;
        }

        let offset = if IS_TAG_PREPENDED {
            TAG_WIDTH_IN_BYTES
        } else {
            0
        };
        self.buf[(self.buffered_count as usize) + offset] = item;
        self.buffered_count += 1;

        Ok(())
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        self.flush().await?;

        self.send_header(0).await?;

        Ok(self.c.close(fin).await?)
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        C,
    > BufferedConsumer
    for Encryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        C,
    >
where
    C: BulkConsumer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    async fn flush(&mut self) -> Result<(), Self::Error> {
        if self.buffered_count != 0 {
            self.send_header(self.buffered_count).await?;

            let total_len = (self.buffered_count as usize) + TAG_WIDTH_IN_BYTES;

            self.key
                .encrypt_inplace(&self.nonce, &[], &mut self.buf[..total_len]);
            increment_nonce(&mut self.nonce).map_err(|()| EncryptionError::NoncesExhausted)?;

            self.c
                .bulk_consume_full_slice(&self.buf[..total_len])
                .await
                .map_err(ConsumeAtLeastError::into_reason)?;

            self.buffered_count = 0;

            self.c.flush().await?;
        }

        Ok(())
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        C,
    > BulkConsumer
    for Encryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        C,
    >
where
    C: BulkConsumer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    async fn expose_slots<'a>(&'a mut self) -> Result<&'a mut [Self::Item], Self::Error>
    where
        Self::Item: 'a,
    {
        if self.buffered_count == 4096 {
            self.flush().await?;
        }

        let offset = if IS_TAG_PREPENDED {
            TAG_WIDTH_IN_BYTES
        } else {
            0
        };
        Ok(&mut self.buf[(self.buffered_count as usize) + offset..4096 + offset])
    }

    async fn consume_slots(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.buffered_count += amount as u16;
        Ok(())
    }
}

/// The possible errors emitted by a decryptor.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecryptionError<ProducerError> {
    /// The inner consumer emitted an error.
    Inner(ProducerError),
    /// Exhausted all possible nonces (happens after sending 2^64 messages).
    NoncesExhausted,
    /// Got bytes that could not be decrypted.
    DecryptionFailure,
    /// A zero length header signaled the end of the stream, but this was not followed by emitting the final item of the stream.
    WeirdEndOfStream,
    /// The peer sent a length header greater than 4096.
    InvalidHeader,
    /// The peer ended the stream without authenticating that via a zero length header.
    UnauthenticatedEndOfStream,
}

impl<ProducerError: core::fmt::Display> core::fmt::Display for DecryptionError<ProducerError> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecryptionError::Inner(err) => err.fmt(f),
            DecryptionError::NoncesExhausted => {
                write!(
                    f,
                    "Exhausted all possible nonces (happens after sending 2^64 messages)."
                )
            }
            DecryptionError::DecryptionFailure => {
                write!(f, "Got some bytes that could not be decrypted.")
            }
            DecryptionError::WeirdEndOfStream => {
                write!(f, "A zero-length header signaled the end of the stream, but this was not followed by emitting the final item of the stream.")
            }
            DecryptionError::InvalidHeader => {
                write!(f, "The peer sent a length header greater than 4096.")
            }
            DecryptionError::UnauthenticatedEndOfStream => {
                write!(f, "The peer ended the stream without authenticating that via a zero length header.")
            }
        }
    }
}

impl<ProducerError: std::error::Error> std::error::Error for DecryptionError<ProducerError> {}

impl<ProducerError> From<ProducerError> for DecryptionError<ProducerError> {
    fn from(value: ProducerError) -> Self {
        DecryptionError::Inner(value)
    }
}

pub struct Decryptor<
    const TAG_WIDTH_IN_BYTES: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
    const NONCE_WIDTH_IN_BYTES: usize,
    const IS_TAG_PREPENDED: bool,
    AEADKey,
    P,
> {
    key: AEADKey,
    nonce: [u8; NONCE_WIDTH_IN_BYTES],
    buf: [u8; TAG_WIDTH_IN_BYTES_PLUS_4096],
    /// Number of decrypted bytes in the buffer.
    buffered_count: u16,
    current_chunk_len: u16,
    p: P,
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        P,
    >
    Decryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        P,
    >
{
    pub fn new(key: AEADKey, inner: P) -> Self {
        Self {
            key,
            nonce: [0; NONCE_WIDTH_IN_BYTES],
            buf: [0; TAG_WIDTH_IN_BYTES_PLUS_4096],
            buffered_count: 0,
            current_chunk_len: 42, // must start as nonzero, otherwise the implementation would consider the stream finished immediately
            p: inner,
        }
    }

    pub fn into_inner(self) -> P {
        self.p
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        P,
    >
    Decryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        P,
    >
where
    P: BulkProducer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    async fn fill_buffer(&mut self) -> Result<(), DecryptionError<P::Error>> {
        // println!(
        //     "start fill; buffered_count {:?}, current_chunk_len {:?}",
        //     self.buffered_count, self.current_chunk_len
        // );
        if self.buffered_count == 0 && self.current_chunk_len != 0 {
            let mut header_buf = [0; TAG_WIDTH_IN_BYTES_PLUS_2];
            self.p
                .bulk_overwrite_full_slice(&mut header_buf)
                .await
                .map_err(|err| match err.reason {
                    Left(_) => DecryptionError::UnauthenticatedEndOfStream,
                    Right(inner_err) => DecryptionError::Inner(inner_err),
                })?;

            self.key
                .decrypt_inplace(&self.nonce, &[], &mut header_buf[..])
                .map_err(|()| DecryptionError::DecryptionFailure)?;
            increment_nonce(&mut self.nonce).map_err(|()| DecryptionError::NoncesExhausted)?;

            let offset = if IS_TAG_PREPENDED {
                TAG_WIDTH_IN_BYTES
            } else {
                0
            };
            let len = u16::from_be_bytes([header_buf[offset], header_buf[offset + 1]]) as usize;

            if len > 4096 {
                return Err(DecryptionError::InvalidHeader);
            } else if len == 0 {
                self.buffered_count = len as u16;
                self.current_chunk_len = len as u16;
                return Ok(());
            }

            self.p
                .bulk_overwrite_full_slice(&mut self.buf[..len + TAG_WIDTH_IN_BYTES])
                .await
                .map_err(|err| match err.reason {
                    Left(_) => DecryptionError::UnauthenticatedEndOfStream,
                    Right(inner_err) => DecryptionError::Inner(inner_err),
                })?;

            self.key
                .decrypt_inplace(&self.nonce, &[], &mut self.buf[..len + TAG_WIDTH_IN_BYTES])
                .map_err(|()| DecryptionError::DecryptionFailure)?;
            increment_nonce(&mut self.nonce).map_err(|()| DecryptionError::NoncesExhausted)?;

            self.buffered_count = len as u16;
            self.current_chunk_len = len as u16;

            // println!(
            //     "end active fill; buffered_count {:?}, current_chunk_len {:?}",
            //     self.buffered_count, self.current_chunk_len
            // );
        }

        Ok(())
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        P,
    > Producer
    for Decryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        P,
    >
where
    P: BulkProducer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    type Item = u8;

    type Final = P::Final;

    type Error = DecryptionError<P::Error>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        if self.current_chunk_len == 0 {
            match self.p.produce().await? {
                Left(_) => return Err(DecryptionError::WeirdEndOfStream),
                Right(fin) => return Ok(Right(fin)),
            }
        }

        // println!(
        //     "pre-fill: chunklen {:?}, bufcount {:?}",
        //     self.current_chunk_len, self.buffered_count
        // );

        if self.buffered_count == 0 {
            self.fill_buffer().await?;
        }

        // println!(
        //     "post-fill: chunklen {:?}, bufcount {:?}, {:?}",
        //     self.current_chunk_len, self.buffered_count, self.buf
        // );

        if self.current_chunk_len == 0 {
            match self.p.produce().await? {
                Left(_) => return Err(DecryptionError::WeirdEndOfStream),
                // Left(item) => {
                //     println!("postfinal byte {:?}", item);
                //     return Err(DecryptionError::WeirdEndOfStream);
                // }
                Right(fin) => return Ok(Right(fin)),
            }
        }

        let offset = if IS_TAG_PREPENDED {
            TAG_WIDTH_IN_BYTES
        } else {
            0
        };
        let item = self.buf[((self.current_chunk_len - self.buffered_count) as usize) + offset];
        // println!(
        //     "produce item at {:?}: {:?}",
        //     ((self.current_chunk_len - self.buffered_count) as usize) + offset,
        //     item
        // );
        self.buffered_count -= 1;

        Ok(Left(item))
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        P,
    > BufferedProducer
    for Decryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        P,
    >
where
    P: BulkProducer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        self.fill_buffer().await?;
        self.p.slurp().await?;
        Ok(())
    }
}

impl<
        const TAG_WIDTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
        const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        AEADKey,
        P,
    > BulkProducer
    for Decryptor<
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
        AEADKey,
        P,
    >
where
    P: BulkProducer<Item = u8>,
    AEADKey: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        if self.current_chunk_len == 0 {
            match self.p.produce().await? {
                Left(_) => return Err(DecryptionError::WeirdEndOfStream),
                Right(fin) => return Ok(Right(fin)),
            }
        }

        if self.buffered_count == 0 {
            self.fill_buffer().await?;
        }

        if self.current_chunk_len == 0 {
            match self.p.produce().await? {
                Left(_) => return Err(DecryptionError::WeirdEndOfStream),
                Right(fin) => return Ok(Right(fin)),
            }
        }

        let offset = if IS_TAG_PREPENDED {
            TAG_WIDTH_IN_BYTES
        } else {
            0
        };
        // println!(
        //     "exposing items from {:?} to {:?}: {:?}",
        //     offset + ((self.current_chunk_len - self.buffered_count) as usize),
        //     (self.current_chunk_len as usize) + offset,
        //     &self.buf[offset + ((self.current_chunk_len - self.buffered_count) as usize)
        //         ..(self.current_chunk_len as usize) + offset]
        // );
        Ok(Left(
            &self.buf[offset + ((self.current_chunk_len - self.buffered_count) as usize)
                ..(self.current_chunk_len as usize) + offset],
        ))
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.buffered_count -= amount as u16;
        Ok(())
    }
}

fn increment_nonce<const NONCE_WIDTH_IN_BYTES: usize>(
    nonce: &mut [u8; NONCE_WIDTH_IN_BYTES],
) -> Result<(), ()> {
    for i in 0..NONCE_WIDTH_IN_BYTES {
        let j = NONCE_WIDTH_IN_BYTES - (i + 1);
        let byte = nonce[j];
        match byte.checked_add(1) {
            Some(new) => {
                nonce[j] = new;
                return Ok(());
            }
            None => {
                nonce[j] = 0;
            }
        }
    }

    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_parameters::*;

    use wb_async_utils::spsc::{self, new_spsc};

    #[test]
    fn empty_stream() {
        let ini_to_res_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (ini_to_res_sender, ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let enc_key = SillyAead(17);

        let mut ini_enc: Encryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Encryptor::new(enc_key.clone(), ini_to_res_sender);
        let mut res_dec: Decryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Decryptor::new(enc_key.clone(), ini_to_res_receiver);

        let ini_data = vec![];
        let ini_data_clone = ini_data.clone();
        let ini_fin = 9;

        smol::block_on(async {
            futures::future::join(
                async {
                    ini_enc.consume_full_slice(&ini_data[..]).await.unwrap();
                    ini_enc.close(ini_fin).await.unwrap();
                },
                async {
                    let mut res_got = vec![0; ini_data_clone.len()];
                    res_dec
                        .overwrite_full_slice(&mut res_got[..])
                        .await
                        .unwrap();
                    assert_eq!(&res_got[..], &ini_data_clone[..]);

                    let res_got_fin = res_dec.produce().await.unwrap().unwrap_right();
                    assert_eq!(res_got_fin, ini_fin);
                },
            )
            .await;
        });
    }

    #[test]
    // just another day for you and me in paradise!
    fn slurp_twice() {
        let ini_to_res_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (ini_to_res_sender, ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let enc_key = SillyAead(17);

        let mut ini_enc: Encryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Encryptor::new(enc_key.clone(), ini_to_res_sender);
        let mut res_dec: Decryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Decryptor::new(enc_key.clone(), ini_to_res_receiver);

        let ini_data = vec![];
        let ini_data_clone = ini_data.clone();
        let ini_fin = 9;

        smol::block_on(async {
            futures::future::join(
                async {
                    ini_enc.consume_full_slice(&ini_data[..]).await.unwrap();
                    ini_enc.close(ini_fin).await.unwrap();
                },
                async {
                    let mut res_got = vec![0; ini_data_clone.len()];

                    res_dec.slurp().await.unwrap();
                    res_dec.slurp().await.unwrap();
                    res_dec
                        .overwrite_full_slice(&mut res_got[..])
                        .await
                        .unwrap();
                    assert_eq!(&res_got[..], &ini_data_clone[..]);

                    let res_got_fin = res_dec.produce().await.unwrap().unwrap_right();
                    assert_eq!(res_got_fin, ini_fin);
                },
            )
            .await;
        });
    }

    #[test]
    fn bulk_consuming_works() {
        let ini_to_res_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (ini_to_res_sender, ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let enc_key = SillyAead(17);

        let mut ini_enc: Encryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Encryptor::new(enc_key.clone(), ini_to_res_sender);
        let mut res_dec: Decryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Decryptor::new(enc_key.clone(), ini_to_res_receiver);

        let ini_data = vec![205, 39];
        let ini_data_clone = ini_data.clone();
        let ini_fin = 9;

        smol::block_on(async {
            futures::future::join(
                async {
                    ini_enc
                        .bulk_consume_full_slice(&ini_data[..])
                        .await
                        .unwrap();
                    ini_enc.close(ini_fin).await.unwrap();
                },
                async {
                    let mut res_got = vec![0; ini_data_clone.len()];

                    res_dec.slurp().await.unwrap();
                    res_dec
                        .overwrite_full_slice(&mut res_got[..])
                        .await
                        .unwrap();
                    assert_eq!(&res_got[..], &ini_data_clone[..]);

                    let res_got_fin = res_dec.produce().await.unwrap().unwrap_right();
                    assert_eq!(res_got_fin, ini_fin);
                },
            )
            .await;
        });
    }

    #[test]
    fn bulk_producing_works() {
        let ini_to_res_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (ini_to_res_sender, ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let enc_key = SillyAead(17);

        let mut ini_enc: Encryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Encryptor::new(enc_key.clone(), ini_to_res_sender);
        let mut res_dec: Decryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Decryptor::new(enc_key.clone(), ini_to_res_receiver);

        let ini_data = vec![205, 39];
        let ini_data_clone = ini_data.clone();
        let ini_fin = 9;

        smol::block_on(async {
            futures::future::join(
                async {
                    ini_enc.consume_full_slice(&ini_data[..]).await.unwrap();
                    ini_enc.close(ini_fin).await.unwrap();
                },
                async {
                    let mut res_got = vec![0; ini_data_clone.len()];

                    res_dec.slurp().await.unwrap();
                    res_dec
                        .bulk_overwrite_full_slice(&mut res_got[..])
                        .await
                        .unwrap();
                    assert_eq!(&res_got[..], &ini_data_clone[..]);

                    let res_got_fin = res_dec.produce().await.unwrap().unwrap_right();
                    assert_eq!(res_got_fin, ini_fin);
                },
            )
            .await;
        });
    }

    #[test]
    fn slurp_in_between_data() {
        let ini_to_res_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (ini_to_res_sender, ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let enc_key = SillyAead(17);

        let mut ini_enc: Encryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Encryptor::new(enc_key.clone(), ini_to_res_sender);
        let mut res_dec: Decryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Decryptor::new(enc_key.clone(), ini_to_res_receiver);

        let ini_data = [5, 6];
        let ini_fin = 9;

        smol::block_on(async {
            futures::future::join(
                async {
                    ini_enc.consume_full_slice(&ini_data[..]).await.unwrap();
                    ini_enc.close(ini_fin).await.unwrap();
                },
                async {
                    assert_eq!(Left(5), res_dec.produce().await.unwrap());
                    res_dec.slurp().await.unwrap();
                    assert_eq!(Left(6), res_dec.produce().await.unwrap());

                    let res_got_fin = res_dec.produce().await.unwrap().unwrap_right();
                    assert_eq!(res_got_fin, ini_fin);
                },
            )
            .await;
        });
    }

    #[test]
    fn flush_in_between_data() {
        let ini_to_res_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (ini_to_res_sender, ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let enc_key = SillyAead(17);

        let mut ini_enc: Encryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Encryptor::new(enc_key.clone(), ini_to_res_sender);
        let mut res_dec: Decryptor<1, 3, 4097, 8, false, SillyAead, _> =
            Decryptor::new(enc_key.clone(), ini_to_res_receiver);

        let ini_data = vec![205, 39];
        let ini_data_clone = ini_data.clone();
        let ini_fin = 9;

        smol::block_on(async {
            futures::future::join(
                async {
                    ini_enc.flush().await.unwrap();
                    ini_enc.consume(ini_data[0]).await.unwrap();
                    ini_enc.flush().await.unwrap();
                    ini_enc.consume(ini_data[1]).await.unwrap();
                    ini_enc.flush().await.unwrap();
                    ini_enc.close(ini_fin).await.unwrap();
                },
                async {
                    let mut res_got = vec![0; ini_data_clone.len()];

                    res_dec.slurp().await.unwrap();
                    res_dec
                        .overwrite_full_slice(&mut res_got[..])
                        .await
                        .unwrap();
                    assert_eq!(&res_got[..], &ini_data_clone[..]);

                    let res_got_fin = res_dec.produce().await.unwrap().unwrap_right();
                    assert_eq!(res_got_fin, ini_fin);
                },
            )
            .await;
        });
    }
}
