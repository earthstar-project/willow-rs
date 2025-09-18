use meadowcap::{
    UnverifiedCommunalCapability, UnverifiedMcCapability, UnverifiedMcEnumerationCapability,
    UnverifiedOwnedCapability,
};
use ufotofu::consumer::IntoVec;
/// This program generates the test vector repository for Willow. Requires that the fuzz tests from which we extract the test vectors have been run. They live in the `fuzz` directory of our willor-rs workspace.
use willow_25::{NamespaceId25, PayloadDigest25, Signature25, SubspaceId25, MCC25, MCL25, MPL25};
use willow_data_model::grouping::Area;
use willow_data_model::{grouping::Range3d, Entry, Path};

use ufotofu_codec::test_vector_generation::{
    generate_test_vectors_absolute, generate_test_vectors_canonic_absolute,
    generate_test_vectors_canonic_relative, generate_test_vectors_relative,
    generate_test_vectors_relative_custom_serialisation,
};
use ufotofu_codec::Encodable;

mod path_extends_path;

fn main() {
    pollster::block_on(async {
        let readme = include_str!("./vector_repo_readme.md");
        write_file_create_parent_dirs("../generated_testvectors/README.md", readme).unwrap();

        //////////////////////////
        // Data Model Encodings //
        //////////////////////////

        generate_test_vectors_absolute::<Path<MCL25, MCC25, MPL25>, _, _>(
            "./fuzz/corpus/testvector_EncodePath",
            "../generated_testvectors/EncodePath",
        )
        .await;

        generate_test_vectors_canonic_absolute::<Path<MCL25, MCC25, MPL25>, _, _>(
            "./fuzz/corpus/testvector_encode_path",
            "../generated_testvectors/encode_path",
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
            "../generated_testvectors/EncodePathRelativePath",
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
            "../generated_testvectors/path_rel_path",
        )
        .await;

        path_extends_path::generate_path_extends_path_vectors(
            "./fuzz/corpus/testvector_EncodePathExtendsPath",
            "../generated_testvectors/EncodePathExtendsPath",
        )
        .await;

        path_extends_path::generate_path_extends_path_canonic_vectors(
            "./fuzz/corpus/testvector_path_extends_path",
            "../generated_testvectors/path_extends_path",
        )
        .await;

        generate_test_vectors_absolute::<
            Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeEntry",
            "../generated_testvectors/EncodeEntry",
        )
        .await;

        generate_test_vectors_canonic_absolute::<
            Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_encode_entry",
            "../generated_testvectors/encode_entry",
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
            "../generated_testvectors/EncodeEntryRelativeEntry",
        )
        .await;

        generate_test_vectors_relative_custom_serialisation::<
            Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>,
            (&NamespaceId25, &Range3d<MCL25, MCC25, MPL25, SubspaceId25>),
            _,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeEntryInNamespace3dRange",
            "../generated_testvectors/EncodeEntryInNamespace3dRange",
            encode_ns_3drange,
        )
        .await;

        generate_test_vectors_relative::<
            Area<MCL25, MCC25, MPL25, SubspaceId25>,
            Area<MCL25, MCC25, MPL25, SubspaceId25>,
            _,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeAreaInArea",
            "../generated_testvectors/EncodeAreaInArea",
        )
        .await;

        generate_test_vectors_canonic_relative::<
            Area<MCL25, MCC25, MPL25, SubspaceId25>,
            Area<MCL25, MCC25, MPL25, SubspaceId25>,
            _,
            _,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_area_in_area",
            "../generated_testvectors/area_in_area",
        )
        .await;

        generate_test_vectors_relative::<
            Range3d<MCL25, MCC25, MPL25, SubspaceId25>,
            Range3d<MCL25, MCC25, MPL25, SubspaceId25>,
            _,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_Encode3dRangeRelative3dRange",
            "../generated_testvectors/Encode3dRangeRelative3dRange",
        )
        .await;

        /////////////////////////
        // Meadowcap Encodings //
        /////////////////////////

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
            "../generated_testvectors/EncodeCommunalCapability",
        )
        .await;

        generate_test_vectors_absolute::<
            UnverifiedOwnedCapability<
                MCL25,
                MCC25,
                MPL25,
                NamespaceId25,
                Signature25,
                SubspaceId25,
                Signature25,
            >,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeOwnedCapability",
            "../generated_testvectors/EncodeOwnedCapability",
        )
        .await;

        generate_test_vectors_absolute::<
            UnverifiedMcCapability<
                MCL25,
                MCC25,
                MPL25,
                NamespaceId25,
                Signature25,
                SubspaceId25,
                Signature25,
            >,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeMcCapability",
            "../generated_testvectors/EncodeMcCapability",
        )
        .await;

        generate_test_vectors_absolute::<
            UnverifiedMcEnumerationCapability<
                NamespaceId25,
                Signature25,
                SubspaceId25,
                Signature25,
            >,
            _,
            _,
        >(
            "./fuzz/corpus/testvector_EncodeMcEnumerationCapability",
            "../generated_testvectors/EncodeMcEnumerationCapability",
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

fn encode_ns_3drange(r: &(NamespaceId25, Range3d<MCL25, MCC25, MPL25, SubspaceId25>)) -> Vec<u8> {
    pollster::block_on(async {
        let mut consumer = IntoVec::new();
        r.0.encode(&mut consumer).await.unwrap();
        r.1.encode(&mut consumer).await.unwrap();
        consumer.into_vec()
    })
}
