use crate::{
    DecodeError, RelativeDecodable, RelativeDecodableCanonic, RelativeEncodable,
    RelativeEncodableKnownSize,
};
use core::fmt::Debug;
use core::num::NonZeroUsize;
use std::boxed::Box;
use std::format;
use ufotofu::producer::{FromSlice, TestProducerBuilder};
use ufotofu::{consumer::TestConsumer, producer::TestProducer};

/// Irrespective of the blocking/yielding and sizes of exposed slots of a consumer, encoding always produces the same encoding.
async fn assert_relative_encoding_does_not_care_about_consumer_details<T, R>(
    r: &R,
    t: &T,
    mut c1: TestConsumer<u8, (), ()>,
    mut c2: TestConsumer<u8, (), ()>,
) where
    T: Debug + Eq + RelativeEncodable<R>,
    R: Debug,
{
    let res1 = t.relative_encode(&mut c1, r).await;
    let res2 = t.relative_encode(&mut c2, r).await;
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
        "The same value produced two different (prefixes of) relative encodings for two different consumers.\n\nValue: {:?}\n\n{status}\n\nRelative To: {:?}\n\n\nFirst Consumer: {:?}\n\nFirst Encoding: {:?}\n\n\nSecond Consumer: {:?}\n\nSecond Encoding: {:?}", t, r, c1, consumed1, c2, consumed2, 
    );
}

async fn assert_relative_distinct_values_produce_distinct_encodings<T, R>(r: &R, t1: &T, t2: &T)
where
    T: Debug + Eq + RelativeEncodable<R>,
    R: Debug,
{
    if t1 != t2 {
        let enc1 = t1.relative_encode_into_vec(r).await;
        let enc2 = t2.relative_encode_into_vec(r).await;
        if enc1 == enc2 {
            panic!("Two nonequal values produced the same relative encoding.\n\nFirst Value: {:?}\n\nSecond Value: {:?}\n\nRelative To: {:?}\n\nEncoding: {:?}", t1, t2, r, enc1);
        }
    }
}

async fn assert_relative_encodings_are_prefix_free<T, R>(r: &R, t1: &T, t2: &T)
where
    T: Debug + Eq + RelativeEncodable<R>,
    R: Debug,
{
    if t1 != t2 {
        let enc1 = t1.relative_encode_into_vec(r).await;
        let enc2 = t2.relative_encode_into_vec(r).await;
        if enc2.starts_with(&enc1[..]) {
            panic!("The relative encoding of value one is a prefix of the encoding of value two.\n\nRelative to: {:?}\n\nFirst Value: {:?}\n\nFirst Encoding: {:?}\n\nSecond Value: {:?}\n\nSecond Encoding: {:?}", r, t1, enc1, t2, enc2);
        } else if enc1.starts_with(&enc2[..]) {
            panic!("The relative encoding of value one is a prefix of the encoding of value two.\n\nRelative To: {:?}\n\nFirst Value: {:?}\n\nFirst Encoding: {:?}\n\nSecond Value: {:?}\n\nSecond Encoding: {:?}", r, t2, enc2, t1, enc1);
        }
    }
}

/// Irrespective of the blocking/yielding and sizes of items slots of a producer, decoding always produces the same encoding.
async fn assert_relative_decoding_does_not_care_about_consumer_details<T, R, ErrR>(
    r: &R,
    potential_encoding: &Box<[u8]>,
    exposed_items_sizes1: Box<[NonZeroUsize]>,
    exposed_items_sizes2: Box<[NonZeroUsize]>,
    yield_pattern1: Box<[bool]>,
    yield_pattern2: Box<[bool]>,
) where
    T: Debug + Eq + RelativeDecodable<R, ErrR>,
    ErrR: Debug + Eq,
    R: Debug,
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
    let res1 = T::relative_decode(&mut p1, r).await;
    let res2 = T::relative_decode(&mut p2, r).await;
    match (res1, res2) {
        (Ok(t1), Ok(t2)) => {
            if t1 != t2 {
                panic!("Relatively decoded nonequal values from the same bytestring because exposed item slot sizes and yield patterns of the producers differed.\n\nRelative To: {:?}\n\nFirst Value: {:?}\n\nSecond Value: {:?}\n\n First TestProducer: {:?}\n\nSecond TestProducer: {:?}", r, t1, t2, p1, p2);
            } else {
                // Yay, this is what it should be!
            }
        }
        (Err(err1), Err(err2)) => {
            if err1 != err2 {
                panic!("Got nonequal errors from relatively decoding the same bytestring because exposed item slot sizes and yield patterns of the producers differed.\n\nRelative To: {:?}\n\nFirst Error: {:?}\n\nSecond Error: {:?}\n\n First TestProducer: {:?}\n\nSecond TestProducer: {:?}", r, err1, err2, p1, p2);
            } else {
                // Yay, this is what it should be!
            }
        }
        (res1, res2) => panic!("Got different results from relatively decoding the same bytestring because exposed item slot sizes and yield patterns of the producers differed.\n\nRelative To: {:?}\n\nFirst Result: {:?}\n\nSecond Result: {:?}\n\n First TestProducer: {:?}\n\nSecond TestProducer: {:?}", r, res1, res2, p1, p2),
    };
}

async fn assert_relative_decoding_reads_no_excessive_bytes<T, R, ErrR>(r: &R, enc: &[u8])
where
    T: Debug + Eq + RelativeDecodable<R, ErrR>,
    ErrR: Debug,
    R: Debug,
{
    let mut p = FromSlice::new(enc);

    if let Ok(t) = T::relative_decode(&mut p, r).await {
        let minimal_enc = p.produced_so_far();
        if minimal_enc.len() > 1 {
            let mut p2 = FromSlice::new(&minimal_enc[..minimal_enc.len() - 1]);

            match T::relative_decode(&mut p2, r).await {
                Err(DecodeError::UnexpectedEndOfInput(())) => {
                    // This is what should happen.
                }
                res => panic!("Removing the final byte of a valid relative encoding and trying to decode again did not yield an UnexpectedEndOfInput error!\n\nRelative To: {:?}\n\nThe Valid Encoding: {:?}\n\nWhat It Decoded To: {:?}\n\nThe Result After Decoding From One Less Byte: {:?}", r, enc, t, res),
            }
        }
    }
}

async fn assert_relative_encoding_then_decoding_yields_the_original<T, R, ErrR>(r: &R, t: &T)
where
    T: RelativeEncodable<R> + RelativeDecodable<R, ErrR> + Eq + Debug,
    ErrR: Debug,
    R: Debug,
{
    let enc = t.relative_encode_into_vec(r).await;
    match T::relative_decode_from_slice(&enc[..], r).await {
        Ok(t2) => {
            if t != &t2 {
                panic!("Relative encoding then relative decoding a value yielded nonequal values.\n\nRelative To: {:?}\n\nOriginal: {:?}\n\nEncoding: {:?}\n\n Decoded: {:?}", r, t, &enc[..], t2);
            }
        }
        Err(err) => {
            panic!("Relative encoding then relative decoding a value resulted in failure to decode.\n\nRelative To: {:?}\n\nOriginal: {:?}\n\nEncoding: {:?}\n\nDecoding Error: {:?}", r, t, &enc[..], err);
        }
    }
}

/// Panics if the input values (which should be generated randomly, so you do not need to know what they mean) certify a violation of any invariant of the [`RelativeEncodable`] or [`RelativeDecodable`] trait.
pub async fn assert_relative_basic_invariants<T, R, ErrR>(
    r: &R,
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
    T: RelativeEncodable<R> + RelativeDecodable<R, ErrR> + Eq + Debug + Clone,
    ErrR: Debug + Eq,
    R: Debug,
{
    assert_relative_encoding_does_not_care_about_consumer_details(r, t1, c1.clone(), c2.clone())
        .await;
    assert_relative_distinct_values_produce_distinct_encodings(r, t1, t2).await;
    assert_relative_encodings_are_prefix_free(r, t1, t2).await;
    assert_relative_decoding_does_not_care_about_consumer_details::<T, R, ErrR>(
        r,
        potential_encoding,
        exposed_items_sizes1,
        exposed_items_sizes2,
        yield_pattern1,
        yield_pattern2,
    )
    .await;
    assert_relative_decoding_reads_no_excessive_bytes::<T, R, ErrR>(r, potential_encoding).await;
    assert_relative_encoding_then_decoding_yields_the_original(r, t1).await;
}

async fn assert_relative_len_of_encoding_is_accurate<T, R>(r: &R, t: &T)
where
    T: Debug + RelativeEncodableKnownSize<R>,
    R: Debug,
{
    let enc = t.relative_encode_into_vec(r).await;

    let claimed_len = t.relative_len_of_encoding(r);

    if enc.len() != claimed_len {
        panic!("relative_len_of_encoding reported an incorrect len.\n\nRelative To: {:?}\n\nValue: {:?}\n\nClaimed Length: {:?}\n\nActual Encoding Length: {:?}\n\nEncoding: {:?}", r, t, claimed_len, enc.len(), &enc[..]);
    }
}

/// Panics if the input values (which should be generated randomly, so you do not need to know what they mean) certify a violation of any invariant of the [`RelativeEncodableKnownSize`] trait.
pub async fn assert_relative_known_size_invariants<T, R>(r: &R, t1: &T)
where
    T: RelativeEncodableKnownSize<R> + Debug,
    R: Debug,
{
    assert_relative_len_of_encoding_is_accurate(r, t1).await;
}

async fn assert_relative_distinct_encodings_do_not_canonically_decode_to_equal_values<
    T,
    R,
    ErrR,
    ErrC,
>(
    r: &R,
    potential_encoding1: &[u8],
    potential_encoding2: &[u8],
) where
    T: Debug + RelativeDecodableCanonic<R, ErrR, ErrC> + Eq,
    R: Debug,
    ErrR: Debug,
    ErrC: Debug + From<ErrR>,
{
    let mut p1 = FromSlice::new(potential_encoding1);
    let mut p2 = FromSlice::new(potential_encoding2);

    if let (Ok(t1), Ok(t2)) = (
        T::relative_decode_canonic(&mut p1, r).await,
        T::relative_decode_canonic(&mut p2, r).await,
    ) {
        if p1.produced_so_far() != p2.produced_so_far() && t1 == t2 {
            panic!("Relative canonically decoding two non-equal bytestrings resulted in equal values.\n\nRelative To: {:?}\n\nFirst Bytestring: {:?}\n\nFirst Decoded: {:?}\n\nSecond Bytestring: {:?}\n\nSecond Decoded: {:?}", r, potential_encoding1, t1, potential_encoding2, t2);
        }
    }
}

async fn assert_relative_canonic_decoding_roundtrips<T, R, ErrR, ErrC>(
    r: &R,
    potential_encoding: &[u8],
) where
    T: Debug + RelativeDecodableCanonic<R, ErrR, ErrC> + RelativeEncodable<R>,
    R: Debug,
    ErrR: Debug,
    ErrC: Debug + From<ErrR>,
{
    let mut p = FromSlice::new(potential_encoding);
    if let Ok(t) = T::relative_decode_canonic(&mut p, r).await {
        let reencoding = t.relative_encode_into_vec(r).await;

        if p.produced_so_far() != &reencoding[..] {
            panic!("Successfully relative canonically decoding a bytestring and then relative reencoding did not yield the original bytestring.\n\nRelative To: {:?}\n\nOriginal bytes: {:?}\n\nDecoded: {:?}\n\nReencoded bytes: {:?}", r, potential_encoding, t, &reencoding[..]);
        }
    }
}

async fn assert_relative_canonic_decoding_specialises_regular_decoding<T, R, ErrR, ErrC>(
    r: &R,
    potential_encoding: &[u8],
) where
    T: Debug + RelativeDecodableCanonic<R, ErrR, ErrC> + Eq,
    R: Debug,
    ErrR: Debug,
    ErrC: Debug + From<ErrR>,
{
    if let Ok(t_canonic) = T::relative_decode_canonic_from_slice(potential_encoding, r).await {
        let res = T::relative_decode_from_slice(potential_encoding, r).await;

        match res {
            Ok(t_general) => {
                if t_canonic != t_general {
                    panic!("Successful relative canonic decoding and successful general decoding did not produce equal values.\n\nRelative To: {:?}\n\nThe Encoding: {:?}\n\nCanonically Decoded: {:?}\n\nGenerally Decoded: {:?}", r, potential_encoding, t_canonic, t_general);
                }
            }
            Err(err) => {
                panic!("Canonic relative decoding suceeded but general decoding failed.\n\nRelative To: {:?}\n\nThe Encoding: {:?}\n\nCanonically Decoded: {:?}\n\nGenerally Decoded Error: {:?}", r, potential_encoding, t_canonic, err);
            }
        }
    }
}

/// Panics if the input values (which should be generated randomly, so you do not need to know what they mean) certify a violation of any invariant of the [`RelativeDecodableCanonic`] trait.
pub async fn assert_relative_canonic_invariants<T, R, ErrR, ErrC>(
    r: &R,
    potential_encoding1: &[u8],
    potential_encoding2: &[u8],
) where
    T: Debug + RelativeDecodableCanonic<R, ErrR, ErrC> + RelativeEncodable<R> + Eq,
    R: Debug,
    ErrR: Debug,
    ErrC: Debug + From<ErrR>,
{
    assert_relative_distinct_encodings_do_not_canonically_decode_to_equal_values::<T, R, ErrR, ErrC>(
        r,
        &potential_encoding1,
        &potential_encoding2,
    )
    .await;
    assert_relative_canonic_decoding_specialises_regular_decoding::<T, R, ErrR, ErrC>(
        r,
        &potential_encoding1,
    )
    .await;
    assert_relative_canonic_decoding_roundtrips::<T, R, ErrR, ErrC>(r, &potential_encoding1).await;
}
