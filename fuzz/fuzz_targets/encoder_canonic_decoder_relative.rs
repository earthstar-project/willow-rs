#![no_main]

use libfuzzer_sys::fuzz_target;

use either::Either::*;

use ufotofu::{consumer::IntoVec, pipe, producer::FromSlice, Producer};

use ufotofu_codec::{DecodeError, RelativeCanonicDecoder, RelativeEncoder};
use willow_fuzz::relative_codec_dummy::XORU16;

// Generate a sequence of u16s. Encode them into a vec, then take a prefix of the resulting bytestring, produce that prefix, and decode it. The result should be the corresponding prefix of the original sequence of u16s, either yielding `()` or an unexpected end of input decoding error depending on the parity of the bytestring prefix.

fuzz_target!(|data: (Box<[XORU16]>, usize, u16)| {
    pollster::block_on(async {
        let (items, cuttoff, relative_to) = data;

        let mut encoder =
            RelativeEncoder::new(IntoVec::with_capacity(items.len() * 2), relative_to);
        assert!(pipe(&mut FromSlice::new(&items[..]), &mut encoder)
            .await
            .is_ok());
        let concatenation_of_encodings = encoder.into_inner().into_vec();

        let cuttoff = core::cmp::min(cuttoff, concatenation_of_encodings.len());

        let mut decoder: RelativeCanonicDecoder<_, XORU16, u16, _, _, _> =
            RelativeCanonicDecoder::new(
                FromSlice::new(&concatenation_of_encodings[..cuttoff]),
                relative_to,
            );
        let num_items = cuttoff / 2; // rounds down
        let should_emit_final_instead_of_error = cuttoff % 2 == 0;

        let mut i = 0;
        loop {
            match decoder.produce().await {
                Ok(Left(decoded)) => {
                    assert_eq!(decoded, items[i]);
                    i += 1;
                }
                Ok(Right(())) => {
                    assert!(i == num_items);
                    assert!(should_emit_final_instead_of_error);
                    break;
                }
                Err(DecodeError::UnexpectedEndOfInput(_)) => {
                    assert!(i == num_items);
                    assert!(!should_emit_final_instead_of_error);
                    break;
                }
                Err(_) => {
                    unreachable!()
                }
            }
        }
    });
});
