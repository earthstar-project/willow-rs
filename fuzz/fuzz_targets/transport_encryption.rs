#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::{
    consumer::{self, BulkConsumerOperation},
    producer::{self, BulkProducerOperation},
    BulkConsumer, BulkProducer, Consumer, Producer,
};
use wb_async_utils::spsc::{self, new_spsc};

fuzz_target!(|data: (
    usize,
    usize,
    Vec<BulkProducerOperation>,
    Vec<BulkProducerOperation>,
    Vec<BulkConsumerOperation>,
    Vec<BulkConsumerOperation>,
    Box<[u8]>,
    Box<[u8]>,
    u16,
    u16
)| {
    pollster::block_on(async {
        let (
            ini_to_res_channel_capacity,
            res_to_ini_channel_capacity,
            ini_pro_ops,
            res_pro_ops,
            ini_con_ops,
            res_con_ops,
            ini_data,
            res_data,
            ini_fin,
            res_fin,
        ) = data;

        let ini_data_len = ini_data.len();
        let ini_data_clone = ini_data.clone();
        let res_data_len = res_data.len();
        let res_data_clone = res_data.clone();

        let ini_to_res_channel_capacity = (ini_to_res_channel_capacity % 5100) + 1;
        let res_to_ini_channel_capacity = (res_to_ini_channel_capacity % 5100) + 1;

        let ini_esk = SillyDhSecretKey(33);
        let ini_epk = SillyDhPublicKey(33);
        let ini_ssk = SillyDhSecretKey(8);
        let ini_spk = SillyDhPublicKey(8);
        let ini_spk_clone = ini_spk.clone();

        let res_esk = SillyDhSecretKey(11);
        let res_epk = SillyDhPublicKey(11);
        let res_ssk = SillyDhSecretKey(56);
        let res_spk = SillyDhPublicKey(56);
        let res_spk_clone = res_spk.clone();

        let protocol_name = b"Nose_XX_foo_bar_baz";
        let prologue = &[1, 2, 3];

        let ini_to_res_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(ini_to_res_channel_capacity));
        let (ini_to_res_sender, ini_to_res_receiver) = new_spsc(&ini_to_res_state);

        let res_to_ini_state: spsc::State<_, u16, ()> =
            spsc::State::new(ufotofu_queues::Fixed::new(res_to_ini_channel_capacity));
        let (res_to_ini_sender, res_to_ini_receiver) = new_spsc(&res_to_ini_state);

        pollster::block_on(futures::future::join(
            async {
                let (_ini_h, hs_ini_rspk, ini_enc, ini_dec) = run_handshake::<
                    1,
                    128,
                    1,
                    1,
                    3,
                    4097,
                    1,
                    false,
                    SillyHash,
                    SillyDhSecretKey,
                    SillyAead,
                    _,
                    _,
                >(
                    true,
                    ini_esk,
                    ini_epk,
                    ini_ssk,
                    ini_spk,
                    protocol_name,
                    prologue,
                    ini_to_res_sender,
                    res_to_ini_receiver,
                )
                .await
                .unwrap();

                assert_eq!(hs_ini_rspk, res_spk_clone);

                let mut ini_enc = consumer::BulkScrambler::new(ini_enc, ini_con_ops);
                let mut ini_dec = producer::BulkScrambler::new(ini_dec, ini_pro_ops);

                futures::future::join(
                    async {
                        ini_enc
                            .bulk_consume_full_slice(&ini_data[..])
                            .await
                            .unwrap();
                        ini_enc.close(ini_fin).await.unwrap();
                    },
                    async {
                        let mut ini_got = vec![0; res_data_len];
                        ini_dec
                            .bulk_overwrite_full_slice(&mut ini_got[..])
                            .await
                            .unwrap();
                        assert_eq!(&ini_got[..], &res_data_clone[..]);

                        let ini_got_fin = ini_dec.produce().await.unwrap().unwrap_right();
                        assert_eq!(ini_got_fin, res_fin);
                    },
                )
                .await;
            },
            async {
                let (_res_h, hs_res_rspk, res_enc, res_dec) = run_handshake::<
                    1,
                    128,
                    1,
                    1,
                    3,
                    4097,
                    1,
                    false,
                    SillyHash,
                    SillyDhSecretKey,
                    SillyAead,
                    _,
                    _,
                >(
                    false,
                    res_esk,
                    res_epk,
                    res_ssk,
                    res_spk,
                    protocol_name,
                    prologue,
                    res_to_ini_sender,
                    ini_to_res_receiver,
                )
                .await
                .unwrap();

                assert_eq!(hs_res_rspk, ini_spk_clone);

                let mut res_enc = consumer::BulkScrambler::new(res_enc, res_con_ops);
                let mut res_dec = producer::BulkScrambler::new(res_dec, res_pro_ops);

                futures::future::join(
                    async {
                        res_enc
                            .bulk_consume_full_slice(&res_data[..])
                            .await
                            .unwrap();
                        res_enc.close(res_fin).await.unwrap();
                    },
                    async {
                        let mut res_got = vec![0; ini_data_len];
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
            },
        ));
    });
});

//////////////
// Copied from the transport_encryption crate, don't want to expose these behind a features just for a fuzz test...
//////////////

use std::convert::Infallible;

use ufotofu_codec::{
    Decodable, DecodableCanonic, DecodableSync, Encodable, EncodableKnownSize, EncodableSync,
};

use willow_transport_encryption::{
    parameters::{AEADEncryptionKey, DiffieHellmanSecretKey, Hashing},
    run_handshake,
};

// The corresponding public key is equal to the secret key.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SillyDhSecretKey(pub u8);

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct SillyDhPublicKey(pub u8);

impl Encodable for SillyDhPublicKey {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer.consume(self.0).await?;

        Ok(())
    }
}

impl EncodableKnownSize for SillyDhPublicKey {
    fn len_of_encoding(&self) -> usize {
        1
    }
}

impl EncodableSync for SillyDhPublicKey {}

impl Decodable for SillyDhPublicKey {
    type ErrorReason = Infallible;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let num = producer.produce_item().await?;

        Ok(SillyDhPublicKey(num))
    }
}

impl DecodableCanonic for SillyDhPublicKey {
    type ErrorCanonic = Infallible;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode(producer).await
    }
}

impl DecodableSync for SillyDhPublicKey {}

impl DiffieHellmanSecretKey for SillyDhSecretKey {
    type PublicKey = SillyDhPublicKey;

    fn dh(&self, pk: &Self::PublicKey) -> Self::PublicKey {
        SillyDhPublicKey(self.0.wrapping_mul(pk.0))
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct SillyAead(pub u8);

impl AEADEncryptionKey<1, 1, false> for SillyAead {
    fn encrypt_inplace(
        &self,
        nonce: &[u8; 1],
        ad: &[u8],
        plaintext_with_additional_space: &mut [u8],
    ) {
        let len = plaintext_with_additional_space.len();
        for i in 0..(len - 1) {
            plaintext_with_additional_space[i] = plaintext_with_additional_space[i]
                .wrapping_add(self.0)
                .wrapping_add(nonce[0]);
        }
        plaintext_with_additional_space[len - 1] = if ad.len() == 0 { self.0 } else { ad[0] };
    }

    fn decrypt_inplace(
        &self,
        nonce: &[u8; 1],
        ad: &[u8],
        cyphertext_with_tag: &mut [u8],
    ) -> Result<(), ()> {
        let len = cyphertext_with_tag.len();
        for i in 0..(len - 1) {
            cyphertext_with_tag[i] = cyphertext_with_tag[i]
                .wrapping_sub(self.0)
                .wrapping_sub(nonce[0]);
        }

        let valid = if ad.len() == 0 {
            cyphertext_with_tag[len - 1] == self.0
        } else {
            cyphertext_with_tag[len - 1] == ad[0]
        };

        if valid {
            Ok(())
        } else {
            Err(())
        }
    }
}

pub struct SillyHash;

impl Hashing<1, 128, SillyAead> for SillyHash {
    fn hash(data: &[u8]) -> [u8; 1] {
        let mut acc = 0u8;
        for byte in data {
            acc = acc.wrapping_add(*byte);
        }
        [acc]
    }

    fn digest_to_aeadkey(digest: &[u8; 1]) -> SillyAead {
        SillyAead(digest[0])
    }
}
