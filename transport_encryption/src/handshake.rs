use std::convert::Infallible;

use ufotofu::{
    consumer::IntoVec, producer::FromSlice, BulkConsumer, BulkProducer, ConsumeAtLeastError,
};
use ufotofu_codec::{DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync};

use crate::{
    encryption::{Decryptor, Encryptor},
    parameters::{AEADEncryptionKey, DiffieHellmanSecretKey, Hashing},
};

/// Runs the handshake for the WGPS. Returns the shared random bytestring, the static public key of the peer, and a producer and consumer which wrap the ones passed to this function and which automatically encrypt and decrypt all bytes.
///
/// Arguments:
///
/// - is_initiator: whether this peer is the initiator or the responder.
/// - esk: the ephemeral secret key of the peer. Must be randomly selected for each new handshake.
/// - epk: the public key corresponding to the `esk`.
/// - ssk: the static secret key of the peer. May be chosen arbitrarily, and will constrain which capabilities can be synced later.
/// - spk: the public key corresponding to the `spk`
/// - protocol_name, prologue: see the spec
/// - c, p: the consumer and producer that represent the channel over which to communicate with the peer.
#[allow(clippy::too_many_arguments)]
pub async fn run_handshake<
    const HASHLEN_IN_BYTES: usize,
    const BLOCKLEN_IN_BYTES: usize,
    const PK_ENCODING_LENGTH_IN_BYTES: usize,
    const TAG_WIDTH_IN_BYTES: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
    const NONCE_WIDTH_IN_BYTES: usize,
    const IS_TAG_PREPENDED: bool,
    H,
    DH,
    AEAD,
    C,
    P,
>(
    is_initiator: bool,
    esk: DH,
    epk: DH::PublicKey,
    ssk: DH,
    spk: DH::PublicKey,
    protocol_name: &[u8],
    prologue: &[u8],
    mut c: C,
    mut p: P,
) -> Result<
    (
        [u8; HASHLEN_IN_BYTES],
        DH::PublicKey,
        Encryptor<
            TAG_WIDTH_IN_BYTES,
            TAG_WIDTH_IN_BYTES_PLUS_2,
            TAG_WIDTH_IN_BYTES_PLUS_4096,
            NONCE_WIDTH_IN_BYTES,
            IS_TAG_PREPENDED,
            AEAD,
            C,
        >,
        Decryptor<
            TAG_WIDTH_IN_BYTES,
            TAG_WIDTH_IN_BYTES_PLUS_2,
            TAG_WIDTH_IN_BYTES_PLUS_4096,
            NONCE_WIDTH_IN_BYTES,
            IS_TAG_PREPENDED,
            AEAD,
            P,
        >,
    ),
    HandshakeError<C::Error, P::Final, P::Error>,
>
where
    DH: DiffieHellmanSecretKey,
    DH::PublicKey: Default + EncodableSync + EncodableKnownSize + DecodableCanonic,
    AEAD: Default,
    H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
    AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    C: BulkConsumer<Item = u8>,
    P: BulkProducer<Item = u8>,
{
    let mut state =
        State::initial_state::<BLOCKLEN_IN_BYTES, H>(esk, epk, ssk, spk, protocol_name, prologue);

    let outcome = if is_initiator {
        state
            .ini_write_first_message::<BLOCKLEN_IN_BYTES, H, C>(&mut c)
            .await
            .map_err(HandshakeError::SendingError)?;

        state
            .ini_read_second_message::<BLOCKLEN_IN_BYTES, PK_ENCODING_LENGTH_IN_BYTES, TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED, H, P>(&mut p)
            .await
            .map_err(HandshakeError::ReceivingError)?;

        state
            .ini_write_third_message::<BLOCKLEN_IN_BYTES, PK_ENCODING_LENGTH_IN_BYTES, TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED, H, C>(&mut c)
            .await
            .map_err(HandshakeError::SendingError)?;

        state.finalise::<BLOCKLEN_IN_BYTES, H>()
    } else {
        state
            .res_read_first_message::<BLOCKLEN_IN_BYTES, PK_ENCODING_LENGTH_IN_BYTES, TAG_WIDTH_IN_BYTES, H, P>(&mut p)
            .await
            .map_err(HandshakeError::ReceivingError)?;

        state
            .res_write_second_message::<BLOCKLEN_IN_BYTES, PK_ENCODING_LENGTH_IN_BYTES, TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED, H, C>(&mut c)
            .await
            .map_err(HandshakeError::SendingError)?;

        state
            .res_read_third_message::<BLOCKLEN_IN_BYTES, PK_ENCODING_LENGTH_IN_BYTES, TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED, H, P>(&mut p)
            .await
            .map_err(HandshakeError::ReceivingError)?;

        state.finalise::<BLOCKLEN_IN_BYTES, H>()
    };

    if is_initiator {
        Ok((
            outcome.h,
            outcome.remote_static_pk,
            Encryptor::new(outcome.aeadk1, c),
            Decryptor::new(outcome.aeadk2, p),
        ))
    } else {
        Ok((
            outcome.h,
            outcome.remote_static_pk,
            Encryptor::new(outcome.aeadk2, c),
            Decryptor::new(outcome.aeadk1, p),
        ))
    }
}

/// The possible errors emitted by an encryptor.
#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeError<ConsumerError, ProducerFinal, ProducerError> {
    /// Sending a handshake message failed because the inner consumer emitted an error.
    SendingError(ConsumerError),
    /// Receiving a handshake message failed.
    ReceivingError(DecodeError<ProducerFinal, ProducerError, DecryptionFailed>),
}

impl<
        ConsumerError: core::fmt::Display,
        ProducerFinal: core::fmt::Display,
        ProducerError: core::fmt::Display,
    > core::fmt::Display for HandshakeError<ConsumerError, ProducerFinal, ProducerError>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandshakeError::SendingError(err) => err.fmt(f),
            HandshakeError::ReceivingError(err) => err.fmt(f),
        }
    }
}

impl<
        ConsumerError: std::error::Error,
        ProducerFinal: std::error::Error,
        ProducerError: std::error::Error,
    > std::error::Error for HandshakeError<ConsumerError, ProducerFinal, ProducerError>
{
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecryptionFailed;

impl core::fmt::Display for DecryptionFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Could not decrypt a handshake message")
    }
}

impl std::error::Error for DecryptionFailed {}

pub(crate) struct State<const HASHLEN_IN_BYTES: usize, DH: DiffieHellmanSecretKey, AEAD> {
    esk: DH,
    epk: DH::PublicKey,
    ssk: DH,
    spk: DH::PublicKey,
    h: [u8; HASHLEN_IN_BYTES],
    ck: [u8; HASHLEN_IN_BYTES],
    repk: DH::PublicKey,
    k: AEAD,
    rspk: DH::PublicKey,
}

impl<const HASHLEN_IN_BYTES: usize, DH, AEAD> State<HASHLEN_IN_BYTES, DH, AEAD>
where
    DH: DiffieHellmanSecretKey,
    DH::PublicKey: Default + EncodableSync + EncodableKnownSize + DecodableCanonic,
    AEAD: Default,
{
    pub fn initial_state<
        const BLOCKLEN_IN_BYTES: usize,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
    >(
        esk: DH,
        epk: DH::PublicKey,
        ssk: DH,
        spk: DH::PublicKey,
        protocol_name: &[u8],
        prologue: &[u8],
    ) -> Self {
        let h = H::hash(protocol_name);
        let ck = h;

        let mut ret = State {
            esk,
            epk,
            ssk,
            spk,
            h,
            ck,
            repk: DH::PublicKey::default(),
            k: AEAD::default(),
            rspk: DH::PublicKey::default(),
        };

        Self::mix_hash::<BLOCKLEN_IN_BYTES, H>(&mut ret.h, prologue);

        ret
    }

    /// Write the first message - sent by the initiator - into the given consumer, and update state accordingly.
    pub async fn ini_write_first_message<
        const BLOCKLEN_IN_BYTES: usize,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        C: BulkConsumer<Item = u8>,
    >(
        &mut self,
        c: &mut C,
    ) -> Result<(), C::Error> {
        let enc_epk = self.epk.encode_into_boxed_slice().await;
        c.bulk_consume_full_slice(&enc_epk[..])
            .await
            .map_err(ConsumeAtLeastError::into_reason)?;
        Self::mix_hash::<BLOCKLEN_IN_BYTES, H>(&mut self.h, &enc_epk[..]);
        Ok(())
    }

    /// Read the first message - received by the responder - from the given producer, and update state accordingly.
    pub async fn res_read_first_message<
        const BLOCKLEN_IN_BYTES: usize,
        const PK_ENCODING_LENGTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES: usize,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        P: BulkProducer<Item = u8>,
    >(
        &mut self,
        p: &mut P,
    ) -> Result<(), DecodeError<P::Final, P::Error, DecryptionFailed>> {
        let mut e = vec![0; PK_ENCODING_LENGTH_IN_BYTES];
        p.bulk_overwrite_full_slice(&mut e[..]).await?;

        self.repk = DH::PublicKey::decode_canonic(&mut FromSlice::new(&e[..]))
            .await
            .map_err(map_slice_err)?;

        Self::mix_hash::<BLOCKLEN_IN_BYTES, H>(&mut self.h, &e[..]);
        Ok(())
    }

    /// Write the second message - sent by the responder - into the given consumer, and update state accordingly.
    pub async fn res_write_second_message<
        const BLOCKLEN_IN_BYTES: usize,
        const PK_ENCODING_LENGTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        C: BulkConsumer<Item = u8>,
    >(
        &mut self,
        c: &mut C,
    ) -> Result<(), C::Error>
    where
        AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    {
        // e
        let enc_epk = self.epk.encode_into_boxed_slice().await;
        c.bulk_consume_full_slice(&enc_epk[..])
            .await
            .map_err(ConsumeAtLeastError::into_reason)?;
        Self::mix_hash::<BLOCKLEN_IN_BYTES, H>(&mut self.h, &enc_epk[..]);

        // ee
        self.mix_key::<BLOCKLEN_IN_BYTES, H>(&self.esk.dh(&self.repk));

        // s
        Self::encrypt_key_then_send_and_hash::<
            BLOCKLEN_IN_BYTES,
            PK_ENCODING_LENGTH_IN_BYTES,
            TAG_WIDTH_IN_BYTES,
            NONCE_WIDTH_IN_BYTES,
            IS_TAG_PREPENDED,
            H,
            C,
        >(&mut self.h, &self.k, &self.spk, c)
        .await?;

        // es
        self.mix_key::<BLOCKLEN_IN_BYTES, H>(&self.ssk.dh(&self.repk));
        Ok(())
    }

    /// Read the second message - received by the initiator - from the given producer, and update state accordingly.
    pub async fn ini_read_second_message<
        const BLOCKLEN_IN_BYTES: usize,
        const PK_ENCODING_LENGTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        P: BulkProducer<Item = u8>,
    >(
        &mut self,
        p: &mut P,
    ) -> Result<(), DecodeError<P::Final, P::Error, DecryptionFailed>>
    where
        AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    {
        // e
        let mut e = vec![0; PK_ENCODING_LENGTH_IN_BYTES];
        p.bulk_overwrite_full_slice(&mut e[..]).await?;

        self.repk = DH::PublicKey::decode_canonic(&mut FromSlice::new(&e[..]))
            .await
            .map_err(map_slice_err)?;

        Self::mix_hash::<BLOCKLEN_IN_BYTES, H>(&mut self.h, &e[..]);

        // ee
        self.mix_key::<BLOCKLEN_IN_BYTES, H>(&self.esk.dh(&self.repk));

        // s
        self.rspk = Self::receive_key_and_hash_then_decrypt::<
            BLOCKLEN_IN_BYTES,
            PK_ENCODING_LENGTH_IN_BYTES,
            TAG_WIDTH_IN_BYTES,
            NONCE_WIDTH_IN_BYTES,
            IS_TAG_PREPENDED,
            H,
            P,
        >(&mut self.h, &self.k, p)
        .await?;

        // es
        self.mix_key::<BLOCKLEN_IN_BYTES, H>(&self.esk.dh(&self.rspk));

        Ok(())
    }

    /// Write the third message - sent by the initiator - into the given consumer, and update state accordingly.
    pub async fn ini_write_third_message<
        const BLOCKLEN_IN_BYTES: usize,
        const PK_ENCODING_LENGTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        C: BulkConsumer<Item = u8>,
    >(
        &mut self,
        c: &mut C,
    ) -> Result<(), C::Error>
    where
        AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    {
        // s
        Self::encrypt_key_then_send_and_hash::<
            BLOCKLEN_IN_BYTES,
            PK_ENCODING_LENGTH_IN_BYTES,
            TAG_WIDTH_IN_BYTES,
            NONCE_WIDTH_IN_BYTES,
            IS_TAG_PREPENDED,
            H,
            C,
        >(&mut self.h, &self.k, &self.spk, c)
        .await?;

        // se
        self.mix_key::<BLOCKLEN_IN_BYTES, H>(&self.ssk.dh(&self.repk));

        Ok(())
    }

    /// Read the third message - received by the responder - from the given producer, and update state accordingly.
    pub async fn res_read_third_message<
        const BLOCKLEN_IN_BYTES: usize,
        const PK_ENCODING_LENGTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        P: BulkProducer<Item = u8>,
    >(
        &mut self,
        p: &mut P,
    ) -> Result<(), DecodeError<P::Final, P::Error, DecryptionFailed>>
    where
        AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    {
        // s
        self.rspk = Self::receive_key_and_hash_then_decrypt::<
            BLOCKLEN_IN_BYTES,
            PK_ENCODING_LENGTH_IN_BYTES,
            TAG_WIDTH_IN_BYTES,
            NONCE_WIDTH_IN_BYTES,
            IS_TAG_PREPENDED,
            H,
            P,
        >(&mut self.h, &self.k, p)
        .await?;

        // se
        self.mix_key::<BLOCKLEN_IN_BYTES, H>(&self.esk.dh(&self.rspk));

        Ok(())
    }

    /// Discard the state and return the final outcome.
    pub fn finalise<
        const BLOCKLEN_IN_BYTES: usize,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
    >(
        self,
    ) -> Outcome<HASHLEN_IN_BYTES, AEAD, DH::PublicKey> {
        let mut tmp_k1 = [0; HASHLEN_IN_BYTES];
        let mut tmp_k2 = [0; HASHLEN_IN_BYTES];
        hkdf::<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD, H>(
            &self.ck,
            &[],
            &mut tmp_k1,
            &mut tmp_k2,
        );

        Outcome {
            h: self.h,
            remote_static_pk: self.rspk,
            aeadk1: H::digest_to_aeadkey(&tmp_k1),
            aeadk2: H::digest_to_aeadkey(&tmp_k2),
        }
    }

    fn mix_hash<
        const BLOCKLEN_IN_BYTES: usize,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
    >(
        h: &mut [u8; HASHLEN_IN_BYTES],
        data: &[u8],
    ) {
        let mut conc = Vec::with_capacity(HASHLEN_IN_BYTES + data.len());
        conc.extend_from_slice(h);
        conc.extend_from_slice(data);
        *h = H::hash(&conc[..]);
    }

    fn mix_key<
        const BLOCKLEN_IN_BYTES: usize,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
    >(
        &mut self,
        input_key_material: &DH::PublicKey,
    ) {
        let encoded_input_key = input_key_material.sync_encode_into_boxed_slice();

        let mut tmp_ck = [0; HASHLEN_IN_BYTES];
        let mut tmp_k = [0; HASHLEN_IN_BYTES];
        hkdf::<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD, H>(
            &self.ck,
            &encoded_input_key[..],
            &mut tmp_ck,
            &mut tmp_k,
        );

        self.ck = tmp_ck;
        self.k = H::digest_to_aeadkey(&tmp_k);
    }

    #[allow(clippy::same_item_push)] // I find the repeated push more clear than computing the correct sizes
    async fn encrypt_key_then_send_and_hash<
        const BLOCKLEN_IN_BYTES: usize,
        const PK_ENCODING_LENGTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        C: BulkConsumer<Item = u8>,
    >(
        h: &mut [u8; HASHLEN_IN_BYTES],
        k: &AEAD,
        pk: &DH::PublicKey,
        c: &mut C,
    ) -> Result<(), C::Error>
    where
        AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    {
        let mut buf = if IS_TAG_PREPENDED {
            let mut buf = Vec::with_capacity(PK_ENCODING_LENGTH_IN_BYTES + TAG_WIDTH_IN_BYTES);
            for _ in 0..TAG_WIDTH_IN_BYTES {
                buf.push(0);
            }
            let mut c = IntoVec::from_vec(buf);
            pk.encode(&mut c).await.unwrap();
            c.into_vec()
        } else {
            let mut c = IntoVec::with_capacity(PK_ENCODING_LENGTH_IN_BYTES + TAG_WIDTH_IN_BYTES);
            pk.encode(&mut c).await.unwrap();
            let mut buf = c.into_vec();
            for _ in 0..TAG_WIDTH_IN_BYTES {
                buf.push(0);
            }
            buf
        };

        let zero_nonce = [0u8; NONCE_WIDTH_IN_BYTES];

        k.encrypt_inplace(&zero_nonce, h, &mut buf[..]);

        c.bulk_consume_full_slice(&buf[..])
            .await
            .map_err(ConsumeAtLeastError::into_reason)?;

        Self::mix_hash::<BLOCKLEN_IN_BYTES, H>(h, &buf[..]);

        Ok(())
    }

    async fn receive_key_and_hash_then_decrypt<
        const BLOCKLEN_IN_BYTES: usize,
        const PK_ENCODING_LENGTH_IN_BYTES: usize,
        const TAG_WIDTH_IN_BYTES: usize,
        const NONCE_WIDTH_IN_BYTES: usize,
        const IS_TAG_PREPENDED: bool,
        H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
        P: BulkProducer<Item = u8>,
    >(
        h: &mut [u8; HASHLEN_IN_BYTES],
        k: &AEAD,
        p: &mut P,
    ) -> Result<DH::PublicKey, DecodeError<P::Final, P::Error, DecryptionFailed>>
    where
        AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    {
        let mut buf = vec![0; PK_ENCODING_LENGTH_IN_BYTES + TAG_WIDTH_IN_BYTES];
        p.bulk_overwrite_full_slice(&mut buf[..]).await?;

        let old_h = *h;
        Self::mix_hash::<BLOCKLEN_IN_BYTES, H>(h, &buf[..]);

        let zero_nonce = [0u8; NONCE_WIDTH_IN_BYTES];
        k.decrypt_inplace(&zero_nonce, &old_h, &mut buf[..])
            .map_err(|_| DecodeError::Other(DecryptionFailed))?;

        return DH::PublicKey::decode_canonic(&mut FromSlice::new(&buf[..]))
            .await
            .map_err(map_slice_err);
    }
}

fn map_slice_err<Final, Producer, Other>(
    err: DecodeError<(), Infallible, Other>,
) -> DecodeError<Final, Producer, DecryptionFailed> {
    match err {
        DecodeError::Other(_) => DecodeError::Other(DecryptionFailed),
        DecodeError::Producer(_) => unreachable!(),
        DecodeError::UnexpectedEndOfInput(_) => panic!(),
    }
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) struct Outcome<const HASHLEN_IN_BYTES: usize, AEAD, DhPublicKey> {
    pub h: [u8; HASHLEN_IN_BYTES],
    pub remote_static_pk: DhPublicKey,
    pub aeadk1: AEAD,
    pub aeadk2: AEAD,
}

/*
* The below is adapted from the rust code generated by [noise explorer](https://noiseexplorer.com/patterns/XX/).
*/

fn hmac<
    const HASHLEN_IN_BYTES: usize,
    const BLOCKLEN_IN_BYTES: usize,
    AEAD,
    H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
>(
    key: &[u8; HASHLEN_IN_BYTES],
    data: &[u8],
    out: &mut [u8; HASHLEN_IN_BYTES],
) {
    let mut ipad = vec![0x36_u8; BLOCKLEN_IN_BYTES];
    let mut opad = vec![0x5c_u8; BLOCKLEN_IN_BYTES];
    for count in 0..key.len() {
        ipad[count] ^= key[count];
        opad[count] ^= key[count];
    }

    ipad.extend_from_slice(data);
    let inner_output = H::hash(&ipad[..]);

    opad.extend_from_slice(&inner_output[..]);
    out.copy_from_slice(&H::hash(&opad[..])[..]);
}

fn hkdf<
    const HASHLEN_IN_BYTES: usize,
    const BLOCKLEN_IN_BYTES: usize,
    AEAD,
    H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
>(
    chaining_key: &[u8; HASHLEN_IN_BYTES],
    input_key_material: &[u8],
    out1: &mut [u8; HASHLEN_IN_BYTES],
    out2: &mut [u8; HASHLEN_IN_BYTES],
) {
    let mut temp_key = [0u8; HASHLEN_IN_BYTES];
    hmac::<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD, H>(
        chaining_key,
        input_key_material,
        &mut temp_key,
    );

    hmac::<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD, H>(&temp_key, &[1u8], out1);

    let mut input_key_material2 = vec![0_u8; HASHLEN_IN_BYTES + 1];
    input_key_material2[..HASHLEN_IN_BYTES].copy_from_slice(out1);
    input_key_material2[HASHLEN_IN_BYTES] = 2;
    hmac::<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD, H>(&temp_key, &input_key_material2[..], out2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_parameters::*;

    use wb_async_utils::spsc::{self, new_spsc};

    #[test]
    fn handshake_produces_equal_results_for_both_peers() {
        let mut ini = State::initial_state::<128, SillyHash>(
            SillyDhSecretKey(33),
            SillyDhPublicKey(33),
            SillyDhSecretKey(8),
            SillyDhPublicKey(8),
            b"Nose_XX_foo_bar_baz",
            &[1, 2, 3],
        );
        let mut res = State::initial_state::<128, SillyHash>(
            SillyDhSecretKey(14),
            SillyDhPublicKey(14),
            SillyDhSecretKey(9),
            SillyDhPublicKey(9),
            b"Nose_XX_foo_bar_baz",
            &[1, 2, 3],
        );

        let ini_to_res_state: spsc::State<_, (), ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (mut ini_to_res_sender, mut ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let res_to_ini_state: spsc::State<_, (), ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(999 /* capacity */));
        let (mut res_to_ini_sender, mut res_to_ini_receiver) = new_spsc(&res_to_ini_state);

        smol::block_on(futures::future::join(
            async {
                ini.ini_write_first_message::<128, SillyHash, _>(&mut ini_to_res_sender)
                    .await
                    .unwrap();
                ini.ini_read_second_message::<128, 1, 1, 8, false, SillyHash, _>(
                    &mut res_to_ini_receiver,
                )
                .await
                .unwrap();
                ini.ini_write_third_message::<128, 1, 1, 8, false, SillyHash, _>(
                    &mut ini_to_res_sender,
                )
                .await
                .unwrap();
            },
            async {
                res.res_read_first_message::<128, 1, 1, SillyHash, _>(&mut ini_to_res_receiver)
                    .await
                    .unwrap();
                res.res_write_second_message::<128, 1, 1, 8, false, SillyHash, _>(
                    &mut res_to_ini_sender,
                )
                .await
                .unwrap();
                res.res_read_third_message::<128, 1, 1, 8, false, SillyHash, _>(
                    &mut ini_to_res_receiver,
                )
                .await
                .unwrap();
            },
        ));

        let ini_outcome = ini.finalise::<128, SillyHash>();
        let res_outcome = res.finalise::<128, SillyHash>();

        assert_eq!(ini_outcome.h, res_outcome.h);
        assert_eq!(ini_outcome.aeadk1, res_outcome.aeadk1);
        assert_eq!(ini_outcome.aeadk2, res_outcome.aeadk2);
        assert_eq!(ini_outcome.remote_static_pk, SillyDhPublicKey(9));
        assert_eq!(res_outcome.remote_static_pk, SillyDhPublicKey(8));
    }
}
