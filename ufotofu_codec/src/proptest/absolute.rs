use crate::{Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize};
use core::fmt::Debug;
use core::num::NonZeroUsize;
use std::boxed::Box;
use std::format;
use ufotofu::producer::{FromSlice, TestProducerBuilder};
use ufotofu::{consumer::TestConsumer, producer::TestProducer};

/// Irrespective of the blocking/yielding and sizes of exposed slots of a consumer, encoding always produces the same encoding.
async fn assert_encoding_does_not_care_about_consumer_details<T>(
    t: &T,
    mut c1: TestConsumer<u8, (), ()>,
    mut c2: TestConsumer<u8, (), ()>,
) where
    T: Debug + Eq + Encodable,
{
    let res1 = t.encode(&mut c1).await;
    let res2 = t.encode(&mut c2).await;
    let consumed1 = c1.consumed();
    let consumed2 = c2.consumed();
    let common_len = core::cmp::min(consumed1.len(), consumed2.len());
    let status = match (res1, res2) {
        (Ok(()), Ok(())) => format!("Neither consumer errored."),
        (Err(()), Ok(())) => format!(
            "First consumer errored after {} bytes, second did not error.",
            consumed1.len()
        ),
        (Ok(()), Err(())) => format!(
            "First consumer did not error, second consumer errored after {} bytes.",
            consumed2.len()
        ),
        (Err(()), Err(())) => format!(
            "First consumer errored after {} bytes, second consumer errored after {} bytes.",
            consumed1.len(),
            consumed2.len()
        ),
    };
    assert_eq!(
        &consumed1[..common_len],
        &consumed2[..common_len],
        "The same value produced two different (prefixes of) encodings for two different consumers.\n\nValue: {:?}\n\n{status}\n\n\nFirst Consumer: {:?}\n\nFirst Encoding: {:?}\n\n\nSecond Consumer: {:?}\n\nSecond Encoding: {:?}", t, c1, consumed1, c2, consumed2, 
    );
}

async fn assert_distinct_values_produce_distinct_encodings<T>(t1: &T, t2: &T)
where
    T: Debug + Eq + Encodable,
{
    if t1 != t2 {
        let enc1 = t1.encode_into_vec().await;
        let enc2 = t2.encode_into_vec().await;
        if enc1 == enc2 {
            panic!("Two nonequal values produced the same encoding.\n\nFirst Value: {:?}\n\nSecond Value: {:?}\n\nEncoding: {:?}", t1, t2, enc1);
        }
    }
}

async fn assert_encodings_are_prefix_free<T>(t1: &T, t2: &T)
where
    T: Debug + Eq + Encodable,
{
    if t1 != t2 {
        let enc1 = t1.encode_into_vec().await;
        let enc2 = t2.encode_into_vec().await;
        if enc2.starts_with(&enc1[..]) {
            panic!("The encoding of value one is a prefix of the encoding of value two.\n\nFirst Value: {:?}\n\nFirst Encoding: {:?}\n\nSecond Value: {:?}\n\nSecond Encoding: {:?}", t1, enc1, t2, enc2);
        } else if enc1.starts_with(&enc2[..]) {
            panic!("The encoding of value one is a prefix of the encoding of value two.\n\nFirst Value: {:?}\n\nFirst Encoding: {:?}\n\nSecond Value: {:?}\n\nSecond Encoding: {:?}", t2, enc2, t1, enc1);
        }
    }
}

/// Irrespective of the blocking/yielding and sizes of items slots of a producer, decoding always produces the same encoding.
async fn assert_decoding_does_not_care_about_consumer_details<T>(
    potential_encoding: &Box<[u8]>,
    exposed_items_sizes1: Box<[NonZeroUsize]>,
    exposed_items_sizes2: Box<[NonZeroUsize]>,
    yield_pattern1: Box<[bool]>,
    yield_pattern2: Box<[bool]>,
) where
    T: Debug + Eq + Decodable,
    T::ErrorReason: Debug + Eq,
{
    let mut p1: TestProducer<u8, (), ()> =
        TestProducerBuilder::new(potential_encoding.clone(), Ok(()))
            .exposed_items_sizes(exposed_items_sizes1)
            .yield_pattern(yield_pattern1)
            .build();
    let mut p2: TestProducer<u8, (), ()> =
        TestProducerBuilder::new(potential_encoding.clone(), Ok(()))
            .exposed_items_sizes(exposed_items_sizes2)
            .yield_pattern(yield_pattern2)
            .build();
    let res1 = T::decode(&mut p1).await;
    let res2 = T::decode(&mut p2).await;
    match (res1, res2) {
        (Ok(t1), Ok(t2)) => {
            if t1 != t2 {
                panic!("Decoded nonequal values from the same bytestring because exposed item slot sizes and yield patterns of the producers differed.\n\nFirst Value: {:?}\n\nSecond Value: {:?}\n\n First TestProducer: {:?}\n\nSecond TestProducer: {:?}", t1, t2, p1, p2);
            } else {
                // Yay, this is what it should be!
            }
        }
        (Err(err1), Err(err2)) => {
            if err1 != err2 {
                panic!("Got nonequal errors from decoding the same bytestring because exposed item slot sizes and yield patterns of the producers differed.\n\nFirst Error: {:?}\n\nSecond Error: {:?}\n\n First TestProducer: {:?}\n\nSecond TestProducer: {:?}", err1, err2, p1, p2);
            } else {
                // Yay, this is what it should be!
            }
        }
        (res1, res2) => panic!("Got different results from decoding the same bytestring because exposed item slot sizes and yield patterns of the producers differed.\n\nFirst Result: {:?}\n\nSecond Result: {:?}\n\n First TestProducer: {:?}\n\nSecond TestProducer: {:?}", res1, res2, p1, p2),
    };
}

async fn assert_decoding_reads_no_excessive_bytes<T>(enc: &[u8])
where
    T: Debug + Eq + Decodable,
    T::ErrorReason: Debug,
{
    let mut p = FromSlice::new(enc);

    if let Ok(t) = T::decode(&mut p).await {
        let minimal_enc = p.produced_so_far();
        if minimal_enc.len() > 1 {
            let mut p2 = FromSlice::new(&minimal_enc[..minimal_enc.len() - 1]);

            match T::decode(&mut p2).await {
                Err(DecodeError::UnexpectedEndOfInput(())) => {
                    // This is what should happen.
                }
                res => panic!("Removing the final byte of a valid encoding and trying to encode again did not yield an UnexpectedEndOfInput error!\n\nThe Valid Encoding: {:?}\n\nWhat It Decoded To: {:?}\n\nThe Result After Decoding From One Less Byte: {:?}", enc, t, res),
            }
        }
    }
}

async fn assert_encoding_then_decoding_yields_the_original<T>(t: &T)
where
    T: Encodable + Decodable + Eq + Debug,
    T::ErrorReason: Debug,
{
    let enc = t.encode_into_vec().await;
    match T::decode_from_slice(&enc[..]).await {
        Ok(t2) => {
            if t != &t2 {
                panic!("Encoding then decoding a value yielded nonequal values.\n\nOriginal: {:?}\n\nEncoding: {:?}\n\n Decoded: {:?}", t, &enc[..], t2);
            }
        }
        Err(err) => {
            panic!("Encoding then decoding a value resulted in failure to decode.\n\nOriginal: {:?}\n\nEncoding: {:?}\n\n Decodeing Error: {:?}", t, &enc[..], err);
        }
    }
}

/// Panics if the input values (which should be generated randomly, so you do not need to know what they mean) certify a violation of any invariant of the [`Encode`] or [`Decode`] trait.
pub async fn assert_basic_invariants<T>(
    t1: &T,
    t2: &T,
    c1: TestConsumer<u8, (), ()>,
    c2: TestConsumer<u8, (), ()>,
    potential_encoding: &Box<[u8]>,
    exposed_items_sizes1: Box<[NonZeroUsize]>,
    exposed_items_sizes2: Box<[NonZeroUsize]>,
    yield_pattern1: Box<[bool]>,
    yield_pattern2: Box<[bool]>,
) where
    T: Encodable + Decodable + Eq + Debug + Clone,
    T::ErrorReason: Debug + Eq,
{
    assert_encoding_does_not_care_about_consumer_details(t1, c1.clone(), c2.clone()).await;
    assert_distinct_values_produce_distinct_encodings(t1, t2).await;
    assert_encodings_are_prefix_free(t1, t2).await;
    assert_decoding_does_not_care_about_consumer_details::<T>(
        potential_encoding,
        exposed_items_sizes1,
        exposed_items_sizes2,
        yield_pattern1,
        yield_pattern2,
    )
    .await;
    assert_decoding_reads_no_excessive_bytes::<T>(potential_encoding).await;
    assert_encoding_then_decoding_yields_the_original(t1).await;
}

async fn assert_len_of_encoding_is_accurate<T>(t: &T)
where
    T: Debug + EncodableKnownSize,
{
    let enc = t.encode_into_vec().await;

    let claimed_len = t.len_of_encoding();

    if enc.len() != claimed_len {
        panic!("len_of_encoding reported an incorrect len.\n\nValue: {:?}\n\nClaimed Length: {:?}\n\nActual Encoding Length: {:?}\n\nEncoding: {:?}", t, claimed_len, enc.len(), &enc[..]);
    }
}

/// Panics if the input values (which should be generated randomly, so you do not need to know what they mean) certify a violation of any invariant of the [`EncodeKnownLength`] trait.
pub async fn assert_known_length_invariants<T>(t1: &T)
where
    T: EncodableKnownSize + Debug,
{
    assert_len_of_encoding_is_accurate(t1).await;
}

async fn assert_distinct_encodings_do_not_canonically_decode_to_equal_values<T>(
    potential_encoding1: &[u8],
    potential_encoding2: &[u8],
) where
    T: Debug + DecodableCanonic + Eq,
{
    let mut p1 = FromSlice::new(potential_encoding1);
    let mut p2 = FromSlice::new(potential_encoding2);

    if let (Ok(t1), Ok(t2)) = (
        T::decode_canonic(&mut p1).await,
        T::decode_canonic(&mut p2).await,
    ) {
        if p1.produced_so_far() != p2.produced_so_far() && t1 == t2 {
            panic!("Canonically decoding two non-equal bytestrings resulted in equal values.\n\nFirst Bytestring: {:?}\n\nFirst Decoded: {:?}\n\nSecond Bytestring: {:?}\n\nSecond Decoded: {:?}", potential_encoding1, t1, potential_encoding2, t2);
        }
    }
}

async fn assert_canonic_decoding_roundtrips<T>(potential_encoding: &[u8])
where
    T: Debug + DecodableCanonic + Encodable,
{
    let mut p = FromSlice::new(potential_encoding);
    if let Ok(t) = T::decode_canonic(&mut p).await {
        let reencoding = t.encode_into_vec().await;

        if p.produced_so_far() != &reencoding[..] {
            panic!("Successfully canonically decoding a bytestring and then reencoding did not yield the original bytestring.\n\nOriginal bytes: {:?}\n\nDecoded: {:?}\n\nReencoded bytes: {:?}", potential_encoding, t, &reencoding[..]);
        }
    }
}

async fn assert_canonic_decoding_specialises_regular_decoding<T>(potential_encoding: &[u8])
where
    T: Debug + DecodableCanonic + Eq,
    T::ErrorReason: Debug,
{
    if let Ok(t_canonic) = T::decode_canonic_from_slice(potential_encoding).await {
        let res = T::decode_from_slice(potential_encoding).await;

        match res {
            Ok(t_general) => {
                if t_canonic != t_general {
                    panic!("Successful canonic decoding and successful general decoding did not produce equal values.\n\nThe Encoding: {:?}\n\nCanonically Decoded: {:?}\n\nGenerally Decoded: {:?}", potential_encoding, t_canonic, t_general);
                }
            }
            Err(err) => {
                panic!("Canonic decoding suceeded but generla decoding failed.\n\nThe Encoding: {:?}\n\nCanonically Decoded: {:?}\n\nGenerally Decoded Error: {:?}", potential_encoding, t_canonic, err);
            }
        }
    }
}

/// Panics if the input values (which should be generated randomly, so you do not need to know what they mean) certify a violation of any invariant of the [`Encode`] or [`Decode`] trait, or of the [`DecodeCanonic`] trait.
pub async fn assert_canonic_invariants<T>(potential_encoding1: &[u8], potential_encoding2: &[u8])
where
    T: Debug + DecodableCanonic + Encodable + Eq,
    T::ErrorReason: Debug,
{
    assert_distinct_encodings_do_not_canonically_decode_to_equal_values::<T>(
        &potential_encoding1,
        &potential_encoding2,
    )
    .await;
    assert_canonic_decoding_specialises_regular_decoding::<T>(&potential_encoding1).await;
    assert_canonic_decoding_roundtrips::<T>(&potential_encoding1).await;
}
