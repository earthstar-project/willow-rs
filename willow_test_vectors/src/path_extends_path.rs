use std::fs::read_dir;
use std::{fs, io};

use arbitrary::{Arbitrary, Unstructured};
use ufotofu::producer::FromSlice;
use ufotofu_codec::Encodable;

use willow_data_model::{decode_path_extends_path, decode_path_extends_path_canonic, Path};

use willow_25::{MCC25, MCL25, MPL25};

pub async fn generate_path_extends_path_vectors<
    P1: AsRef<std::path::Path>,
    P2: AsRef<std::path::Path>,
>(
    corpus_dir: P1,
    out_dir: P2,
) {
    let mut i = 0;

    for corpus_file in read_dir(corpus_dir).unwrap() {
        let data = fs::read(corpus_file.unwrap().path()).unwrap();
        single_test_vector_path_extends_path(&data[..], out_dir.as_ref(), i).await;
        i += 1;
    }
}

/// Takes the bytes of a corpus file of a `fuzz_relative_corpus` macro, the directory in which to place test vector data, and a counter to number the generated vector, and writes the test vector data to the file system.
async fn single_test_vector_path_extends_path<P: AsRef<std::path::Path>>(
    unstructured_bytes: &[u8],
    out_dir: P,
    count: usize,
) {
    let mut u = Unstructured::new(unstructured_bytes);
    let (bytes, r): (Box<[u8]>, Path<MCL25, MCC25, MPL25>) = Arbitrary::arbitrary(&mut u).unwrap();

    match decode_path_extends_path(&mut FromSlice::new(&bytes[..]), &r).await {
        Ok(decoded) => {
            let mut path_input = out_dir.as_ref().join("yay");
            path_input.push(count.to_string());
            write_file_create_parent_dirs(&path_input, bytes).unwrap();

            let mut path_input_rel = out_dir.as_ref().join("yay_relative_to");
            path_input_rel.push(count.to_string());
            write_file_create_parent_dirs(&path_input_rel, r.encode_into_vec().await).unwrap();

            let pair = EncodedPair {
                actual_value: decoded,
                relative_to: r,
            };

            let mut path_debug_representation = out_dir.as_ref().join("dbg");
            path_debug_representation.push(count.to_string());
            write_file_create_parent_dirs(&path_debug_representation, format!("{:#?}", pair))
                .unwrap();

            let mut path_reencoded = out_dir.as_ref().join("reencoded");
            path_reencoded.push(count.to_string());
            write_file_create_parent_dirs(
                &path_reencoded,
                pair.actual_value.encode_into_vec().await,
            )
            .unwrap();
        }
        Err(reason) => {
            let mut path_input = out_dir.as_ref().join("nay");
            path_input.push(count.to_string());
            write_file_create_parent_dirs(&path_input, bytes).unwrap();

            let mut path_nay_reason = out_dir.as_ref().join("nay_reason");
            path_nay_reason.push(count.to_string());
            write_file_create_parent_dirs(&path_nay_reason, format!("{:#?}", reason)).unwrap();
        }
    }
}

pub async fn generate_path_extends_path_canonic_vectors<
    P1: AsRef<std::path::Path>,
    P2: AsRef<std::path::Path>,
>(
    corpus_dir: P1,
    out_dir: P2,
) {
    let mut i = 0;

    for corpus_file in read_dir(corpus_dir).unwrap() {
        let data = fs::read(corpus_file.unwrap().path()).unwrap();
        single_test_vector_path_extends_path_canonic(&data[..], out_dir.as_ref(), i).await;
        i += 1;
    }
}

/// Takes the bytes of a corpus file of a `fuzz_relative_corpus` macro, the directory in which to place test vector data, and a counter to number the generated vector, and writes the test vector data to the file system.
async fn single_test_vector_path_extends_path_canonic<P: AsRef<std::path::Path>>(
    unstructured_bytes: &[u8],
    out_dir: P,
    count: usize,
) {
    let mut u = Unstructured::new(unstructured_bytes);
    let (bytes, r): (Box<[u8]>, Path<MCL25, MCC25, MPL25>) = Arbitrary::arbitrary(&mut u).unwrap();

    match decode_path_extends_path_canonic(&mut FromSlice::new(&bytes[..]), &r).await {
        Ok(decoded) => {
            let mut path_input = out_dir.as_ref().join("yay");
            path_input.push(count.to_string());
            write_file_create_parent_dirs(&path_input, bytes).unwrap();

            let mut path_input_rel = out_dir.as_ref().join("yay_relative_to");
            path_input_rel.push(count.to_string());
            write_file_create_parent_dirs(&path_input_rel, r.encode_into_vec().await).unwrap();

            let pair = EncodedPair {
                actual_value: decoded,
                relative_to: r,
            };

            let mut path_debug_representation = out_dir.as_ref().join("dbg");
            path_debug_representation.push(count.to_string());
            write_file_create_parent_dirs(&path_debug_representation, format!("{:#?}", pair))
                .unwrap();

            let mut path_reencoded = out_dir.as_ref().join("reencoded");
            path_reencoded.push(count.to_string());
            write_file_create_parent_dirs(
                &path_reencoded,
                pair.actual_value.encode_into_vec().await,
            )
            .unwrap();
        }
        Err(reason) => {
            let mut path_input = out_dir.as_ref().join("nay");
            path_input.push(count.to_string());
            write_file_create_parent_dirs(&path_input, bytes).unwrap();

            let mut path_nay_reason = out_dir.as_ref().join("nay_reason");
            path_nay_reason.push(count.to_string());
            write_file_create_parent_dirs(&path_nay_reason, format!("{:#?}", reason)).unwrap();
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct EncodedPair<T, R> {
    actual_value: T,
    relative_to: R, // This is only there for the Debug impl.
}

fn write_file_create_parent_dirs<P, C>(file: P, contents: C) -> io::Result<()>
where
    P: AsRef<std::path::Path>,
    C: AsRef<[u8]>,
{
    let prefix = file.as_ref().parent().unwrap();
    std::fs::create_dir_all(prefix).unwrap();

    fs::write(file, contents)
}
