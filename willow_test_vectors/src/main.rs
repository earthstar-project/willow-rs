use meadowcap::UnverifiedCommunalCapability;
/// This program generates the test vector repository for Willow. Requires that the fuzz tests from which we extract the test vectors have been run. They live in the `fuzz` directory of our willor-rs workspace.
use willow_25::{NamespaceId25, PayloadDigest25, Signature25, SubspaceId25, MCC25, MCL25, MPL25};
use willow_data_model::{Entry, Path};

use ufotofu_codec::test_vector_generation::{
    generate_test_vectors_absolute, generate_test_vectors_canonic_absolute,
    generate_test_vectors_canonic_relative, generate_test_vectors_relative,
};

mod path_extends_path;

fn main() {
    pollster::block_on(async {
        let readme = include_str!("./vector_repo_readme.md");
        write_file_create_parent_dirs("./generated_testvectors/README.md", readme).unwrap();

        generate_test_vectors_absolute::<Path<MCL25, MCC25, MPL25>, _, _>(
            "./fuzz/corpus/testvector_EncodePath",
            "./generated_testvectors/EncodePath",
        )
        .await;

        generate_test_vectors_canonic_absolute::<Path<MCL25, MCC25, MPL25>, _, _>(
            "./fuzz/corpus/testvector_encode_path",
            "./generated_testvectors/encode_path",
        )
        .await;

        generate_test_vectors_relative::<
            Path<MCL25, MCC25, MPL25>,
            Path<MCL25, MCC25, MPL25>,
            _,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodePathRelativePath",
            "./generated_testvectors/EncodePathRelativePath",
        )
        .await;

        generate_test_vectors_canonic_relative::<
            Path<MCL25, MCC25, MPL25>,
            Path<MCL25, MCC25, MPL25>,
            _,
            _,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_path_rel_path",
            "./generated_testvectors/path_rel_path",
        )
        .await;

        path_extends_path::generate_path_extends_path_vectors(
            "./fuzz/corpus/testvector_EncodePathExtendsPath",
            "./generated_testvectors/EncodePathExtendsPath",
        )
        .await;

        path_extends_path::generate_path_extends_path_canonic_vectors(
            "./fuzz/corpus/testvector_path_extends_path",
            "./generated_testvectors/path_extends_path",
        )
        .await;

        generate_test_vectors_absolute::<
            Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeEntry",
            "./generated_testvectors/EncodeEntry",
        )
        .await;

        generate_test_vectors_canonic_absolute::<
            Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_encode_entry",
            "./generated_testvectors/encode_entry",
        )
        .await;

        generate_test_vectors_relative::<
            Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>,
            Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>,
            _,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeEntryRelativeEntry",
            "./generated_testvectors/EncodeEntryRelativeEntry",
        )
        .await;

        generate_test_vectors_absolute::<
            UnverifiedCommunalCapability<
                MCL25,
                MCC25,
                MPL25,
                NamespaceId25,
                SubspaceId25,
                Signature25,
            >,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeCommunalCapability",
            "./generated_testvectors/EncodeCommunalCapability",
        )
        .await;
    });
}

fn write_file_create_parent_dirs<P, C>(file: P, contents: C) -> std::io::Result<()>
where
    P: AsRef<std::path::Path>,
    C: AsRef<[u8]>,
{
    let prefix = file.as_ref().parent().unwrap();
    std::fs::create_dir_all(prefix).unwrap();

    std::fs::write(file, contents)
}
