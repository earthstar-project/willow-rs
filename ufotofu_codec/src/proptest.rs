use core::convert::Infallible;
use core::fmt::Debug;
use std::format;

use ufotofu::consumer::TestConsumer;

use crate::Encodable;

/// Irrespective of the blocking/yielding and sizes of exposed slots of a consumer, encoding always produces the same encoding.
async fn assert_encoding_does_not_care_about_consumer_details<T>(
    t: &T,
    mut c1: TestConsumer<u8, Infallible, ()>,
    mut c2: TestConsumer<u8, Infallible, ()>,
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

// async fn assert_distinct_values_produce_distinct_encodings<T>(
//     t1: &T,
//     t2: &T,
//     c: TestConsumer<u8, Infallible, ()>,
// ) where
//     T: Eq + Encodable,
// {
//     if t1 != t2 {
//         let mut c1 = c.clone();
//         let mut c2 = c;

//         if t1.encode(&mut c1).await != t2.encode(&mut c2).await {
//             false
//         } else {
//             c1 == c2
//         }
//     }
// }

// async fn assert_encodings_are_prefix_free<T>(t1: &T, t2: &T, c: TestConsumer<u8, Infallible, ()>)
// where
//     T: Eq + Encodable,
// {
//     if t1 == t2 {
//         true
//     } else {
//         let mut c1 = c.clone();
//         let mut c2 = c;

//         if t1.encode(&mut c1).await != t2.encode(&mut c2).await {
//             false
//         } else {
//             c1 == c2
//         }
//     }
// }
