use ufotofu::{Consumer, Producer};
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    Path,
};

mod capable_aois;
mod salted_hashes;

// /// This function is the entry point to our implementation of private interest overlap detection.
// ///
// /// While the internals are somewhat complex, the outer interface of PIO is fairly simple: peers exchange PioBindReadCapability messages. Each of these is essentially an AreaOfInterest together with a read capability for that AoI.
// ///
// /// This function returns:
// ///
// /// - a [`MyAoiInput`], where you can put in the AoIs you'd like to sync (with backpressure applied based on the other peer's guarantees on the OverlapChannel),
// /// - a [`OverlappingAoiOutput`], a producer which emits the AoIs submitted both by the other peer and yourself for which there is an overlap, and
// /// - an [`AoiHandles`], which lets you resolve your and your peer's read capability handles to Aois.
// ///
// /// Internally, everything undergoes a bunch of cryptographic verification, and resource-handle-based backpressure.
// ///
// /// In the WGPS, peers can not only *submit* AoIs but also *revoke* them again (via ResourceHandleFree messages). This implementation punts on that functionality. You cannot undeclare any interest, and all requests by the other peer to remove an AoI will be ignored. This makes us shitty peers, and long-term this functionality should be implemented.
// pub(crate) async fn pio<
//     const SALT_LENGTH: usize,
//     const INTEREST_HASH_LENGTH: usize,
//     const MCL: usize,
//     const MCC: usize,
//     const MPL: usize,
//     N,
//     S,
//     ReadCap,
// >() -> (
//     MyAoiInput<MCL, MCC, MPL, N, S, ReadCap>,
//     OverlappingAoiOutput,
//     AoiHandles,
// )
// where
//     ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
// {
//     todo!()
// }

// /// This is where you submit your own AoIs. More specifically, you asynchronously (backpressure based on the OverlapChannel) submit [`CapableAoi`]s. When a submitted AoI overlaps an AoI transmitted by the peer, both are emitted on the [`OverlappingAoiOutput`] corresponding to self.
// //
// // Poi works based on PrivateInterests, not CapableAois. This struct takes care of the conversion.
// //
// // When given a CapableAoi C, it computes the corresponding PrivateInterest P. Now, there are three cases:
// //
// // - P is completely fresh, it has not yet been submitted to the poi process. In this case, we send a PioBindHash message and then proceed to the next case.
// // - P has already been submitted to the poi process, but we have not (yet) detected an overlap. If (or when) we do detect an overlap for P, we need to emit C on the corresponding `OverlappingAoiOutput`. To be able to do so, we add C to a mapping from P to all the CapableAois that had P as their PrivateInterest. When an overlap with P is detected, we can purge the CapableAois from the mapping, keeping only the information that P has been matched (we keep this information to be able to detect and correctly handle the following third case).
// // - P has already been submitted to the poi process, and we have already detected an overlap. We can immediately emit C on the `OverlappingAoiOutput` and forget about it.
// //
// // In summary, we need to maintain a `Map<PrivateInterest, Option<Set<CapableAoi>>>` (where `None` indicates that we have already detected an overlap).
// pub(crate) struct MyAoiInput<const MCL: usize, const MCC: usize, const MPL: usize, N, S, ReadCap>;

// pub(crate) struct OverlappingAoiOutput;

// pub(crate) struct AoiHandles;

// /// The information you submit to pio detection (basically the information you need for a `PioBindReadCapability` message).
// pub(crate) struct CapableAoi<const MCL: usize, const MCC: usize, const MPL: usize, ReadCap>
// // where
// //     ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
// {
//     /// The read capability for the area one is interested in.
//     capability: ReadCap,
//     /// The max_count of the AreaOfInterest that the sender wants to sync.
//     max_count: u64,
//     /// The max_size of the AreaOfInterest that the sender wants to sync.
//     max_size: u64,
//     max_payload_power: u8,
// }

/// The semantics that a valid read capability must provide to be usable with the WGPS.
pub trait ReadCapability<const MCL: usize, const MCC: usize, const MPL: usize> {
    type Receiver;
    type NamespaceId;
    type SubspaceId;

    fn granted_area(&self) -> Area<MCL, MCC, MPL, Self::SubspaceId>;
    fn granted_namespace(&self) -> Self::NamespaceId;
}

/// The semantics that a valid enumeration capability must provide to be usable with the WGPS.
pub trait EnumerationCapability {
    type Receiver;
    type NamespaceId;

    fn granted_namespace(&self) -> Self::NamespaceId;
    fn receiver(&self) -> Self::Receiver;
}
