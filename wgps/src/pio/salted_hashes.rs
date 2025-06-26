use std::collections::HashMap;

use multimap::MultiMap;
use willow_data_model::{NamespaceId, PrivateInterest, SubspaceId};

use crate::data_handles::handle_store::{HandleStore, HashMapHandleStore};

/// Tell this struct about your own PrivateInterests and about the salted hashes you receive from the other peer, and this struct tells you when there are overlaps. How nice of it. Does not perform any IO, only implements the logic of the core layer of pio.
#[derive(Debug)]
pub(crate) struct HashRegistry<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
> {
    my_salt: [u8; SALT_LENGTH],
    h: fn(&PrivateInterest<MCL, MCC, MPL, N, S>, &[u8; SALT_LENGTH]) -> [u8; INTEREST_HASH_LENGTH],
    my_handles: HashMapHandleStore<MyHandleInfo<MCL, MCC, MPL, N, S>>,
    my_local_hashes: MultiMap<[u8; INTEREST_HASH_LENGTH], LocalHashInfo>,
    their_handles: HashMapHandleStore<bool>,
    their_hashes: HashMap<
        [u8; INTEREST_HASH_LENGTH],
        (
            u64,  /* handles in self.their_handles */
            bool, /* actually interested */
        ),
    >,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId,
        S: SubspaceId,
    > HashRegistry<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S>
{
    pub(crate) fn new(
        my_salt: [u8; SALT_LENGTH],
        h: fn(
            &PrivateInterest<MCL, MCC, MPL, N, S>,
            &[u8; SALT_LENGTH],
        ) -> [u8; INTEREST_HASH_LENGTH],
    ) -> Self {
        Self {
            my_salt,
            h,
            my_handles: HashMapHandleStore::new(u64::MAX),
            my_local_hashes: MultiMap::new(),
            their_hashes: HashMap::new(),
            their_handles: HashMapHandleStore::new(u64::MAX),
        }
    }

    fn their_salt(&self) -> [u8; SALT_LENGTH] {
        let mut salt = self.my_salt.clone();
        for byte in salt.iter_mut() {
            *byte = *byte ^ 255;
        }
        return salt;
    }

    /// Call this when you want to add a PrivateInterest. Returns the one or two salted hashes to send to the peer. They must be sent to the peer in the same order as this method call, otherwise resource handles will be mixed up and everything breaks. The first hash must be sent with associated bool `true`, the second hash (if there is one) must be sent with associated bool `false`. Also returns any detected matches.
    pub(crate) fn submit_private_interest(
        &mut self,
        p: &PrivateInterest<MCL, MCC, MPL, N, S>,
    ) -> (
        [u8; INTEREST_HASH_LENGTH],
        Option<[u8; INTEREST_HASH_LENGTH]>,
        Vec<Overlap>,
    ) {
        let mut overlaps = vec![];

        // The two hashes to send to the peer (first has associated bool true, the second, optional one has associated bool false).
        let fst = (self.h)(p, &self.my_salt);
        let snd = if p.subspace_id().is_any() {
            None
        } else {
            Some((self.h)(&p.relax(), &self.my_salt))
        };

        // Add handles to handle store for the one or two hash(es) we send.
        let interesting_handle = self
            .my_handles
            .bind(MyHandleInfo::Interesting(MyInterestingHandleInfo {
                private_interest: p.clone(),
            }))
            .unwrap();

        if let Some(_) = snd {
            let _ = self.my_handles.bind(MyHandleInfo::Uninteresting {
                interesting_handle: interesting_handle,
            });
        }

        // Compute local hashes and store them.
        // If they match a hash bound by the other peer, note the overlap.
        for prefix in p.path().all_prefixes() {
            let is_original = prefix.component_count() == p.path().component_count();
            let prefix_interest =
                PrivateInterest::new(p.namespace_id().clone(), p.subspace_id().clone(), prefix);

            let fst_hash = (self.h)(&prefix_interest, &self.their_salt());

            self.my_local_hashes.insert(
                fst_hash,
                LocalHashInfo {
                    not_a_relaxation: true,
                    has_original_path: is_original,
                    interesting_handle: interesting_handle,
                },
            );

            if let Some((their_handle, they_are_actually_interested)) =
                self.their_hashes.get(&fst_hash)
            {
                overlaps.push(Overlap {
                    actually_equal: is_original && *they_are_actually_interested,
                    awkward: !they_are_actually_interested,
                    my_interesting_handle: interesting_handle,
                    their_handle: *their_handle,
                });
            }

            if p.subspace_id().is_any() {
                let relaxation = prefix_interest.relax();
                let snd_hash = (self.h)(&relaxation, &self.their_salt());

                self.my_local_hashes.insert(
                    snd_hash,
                    LocalHashInfo {
                        not_a_relaxation: false,
                        has_original_path: is_original,
                        interesting_handle: interesting_handle,
                    },
                );

                if let Some((their_handle, they_are_actually_interested)) =
                    self.their_hashes.get(&snd_hash)
                {
                    if *they_are_actually_interested {
                        overlaps.push(Overlap {
                            actually_equal: false,
                            awkward: false,
                            my_interesting_handle: interesting_handle,
                            their_handle: *their_handle,
                        });
                    }
                }
            }
        }

        return (fst, snd, overlaps);
    }

    /// Call this whenever the peer sends a PioBindHash message. Returns any detected matches.
    fn add_their_incoming_binding(
        &mut self,
        hash: [u8; INTEREST_HASH_LENGTH],
        actually_interested: bool,
    ) -> Vec<Overlap> {
        let mut overlaps = vec![];

        let their_handle = self.their_handles.bind(actually_interested).unwrap();
        self.their_hashes
            .insert(hash, (their_handle, actually_interested));

        if let Some(matches) = self.my_local_hashes.get_vec(&hash) {
            for info in matches.iter() {
                // Overlap does not count if neither peer is actually interested.
                if info.not_a_relaxation || actually_interested {
                    overlaps.push(Overlap {
                        actually_equal: info.not_a_relaxation && actually_interested,
                        awkward: !actually_interested,
                        my_interesting_handle: info.interesting_handle,
                        their_handle,
                    });
                }
            }
        }

        return overlaps;
    }

    /// Only call this when we know we have bound info, this unwarps the retrieved value.
    fn get_interesting_handle_info(
        &self,
        my_handle: u64,
    ) -> &MyInterestingHandleInfo<MCL, MCC, MPL, N, S> {
        let my_handle_info = self.my_handles.get(my_handle).unwrap().unwrap();

        match my_handle_info {
            MyHandleInfo::Uninteresting { interesting_handle } => {
                return self.get_interesting_handle_info(*interesting_handle)
            }
            MyHandleInfo::Interesting(interesting_info) => interesting_info,
        }
    }
}

/// Information to which to map the locally computed hashes.
#[derive(Debug)]
struct LocalHashInfo {
    not_a_relaxation: bool,
    /// False when the hash is for a strict prefix.
    has_original_path: bool,
    /// The resource handle we sent with boolean `true` that correspond to this hash.
    interesting_handle: u64,
}

/// Information to associate with the OverlapHandles that we bind.
#[derive(Debug)]
enum MyHandleInfo<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
> {
    /// For the handles we sent with boolean `false`, all we store is the corresponding handle which we sent with the boolean `true`.
    Uninteresting { interesting_handle: u64 },
    /// A handle we sent with boolean `true`.
    Interesting(MyInterestingHandleInfo<MCL, MCC, MPL, N, S>),
}

#[derive(Debug)]
pub struct MyInterestingHandleInfo<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
> {
    pub private_interest: PrivateInterest<MCL, MCC, MPL, N, S>,
}

/// Information to report when an overlap has been detected.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Overlap {
    /// The interest of ours and of theirs that resulted in this overlap were equal.
    pub actually_equal: bool,
    /// This overlap corresponds to an awkward pair.
    pub awkward: bool,
    /// The resource handle we sent with boolean `true` that correspond to this hash.
    pub my_interesting_handle: u64,
    pub their_handle: u64,
}

#[cfg(test)]
mod tests {
    use std::hash::{DefaultHasher, Hash, Hasher};

    use willow_25::{NamespaceId25, SubspaceId25};
    use willow_data_model::{grouping::AreaSubspace, Path};

    use super::*;

    #[test]
    fn spec_examples() {
        let my_salt = [0u8; 16];
        let their_salt = [255u8; 16];
        let mut my_registry: HashRegistry<16, 32, 1024, 1024, 1024, NamespaceId25, SubspaceId25> =
            HashRegistry::new(my_salt, h);
        let mut their_registry: HashRegistry<
            16,
            32,
            1024,
            1024,
            1024,
            NamespaceId25,
            SubspaceId25,
        > = HashRegistry::new(their_salt, h);

        let namespace = NamespaceId25::new_communal();
        let (subspace, _) = SubspaceId25::new();
        let (other_subspace, _) = SubspaceId25::new();

        // me: gemma, a   they: gemma, a
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![Overlap {
                actually_equal: true,
                awkward: false,
                my_interesting_handle: 0,
                their_handle: 0,
            }],
            vec![],
        );

        // me: any, b   they: any, c
        assert_mine(&mut my_registry, &namespace, None, &["b"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["c"],
            vec![],
            vec![],
        );

        // me: any, d   they: any, d/e
        assert_mine(&mut my_registry, &namespace, None, &["d"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["d", "e"],
            vec![],
            vec![],
        );

        // me: any, f   they: gemma, g
        assert_mine(&mut my_registry, &namespace, None, &["f"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["g"],
            vec![],
            vec![],
        );

        // me: any, h   they: gemma, h/i
        assert_mine(&mut my_registry, &namespace, None, &["h"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["h", "i"],
            vec![],
            vec![],
        );

        // me: any, j/k   they: gemma, j   awkward!
        assert_mine(&mut my_registry, &namespace, None, &["j", "k"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["j"],
            vec![],
            vec![Overlap {
                actually_equal: false,
                awkward: true,
                my_interesting_handle: 6,
                their_handle: 9,
            }],
        );

        // me: gemma, l   they: gemma, m
        assert_mine(&mut my_registry, &namespace, Some(&subspace), &["l"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["m"],
            vec![],
            vec![],
        );

        // me: gemma, n   they: gemma, n/o
        assert_mine(&mut my_registry, &namespace, Some(&subspace), &["n"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["n", "o"],
            vec![],
            vec![],
        );

        // me: gemma, p   they: dalton, q
        assert_mine(&mut my_registry, &namespace, Some(&subspace), &["p"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&other_subspace),
            &["q"],
            vec![],
            vec![],
        );

        // me: gemma, r   they: dalton, r/s
        assert_mine(&mut my_registry, &namespace, Some(&subspace), &["r"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&other_subspace),
            &["r", "s"],
            vec![],
            vec![],
        );
    }

    fn assert_mine(
        registry: &mut HashRegistry<16, 32, 1024, 1024, 1024, NamespaceId25, SubspaceId25>,
        namespace: &NamespaceId25,
        subspace: Option<&SubspaceId25>,
        path_components: &[&'static str],
        expected: Vec<Overlap>,
    ) {
        let (_, _, overlaps) = registry.submit_private_interest(&PrivateInterest::new(
            namespace.clone(),
            match subspace {
                Some(id) => AreaSubspace::Id(id.clone()),
                None => AreaSubspace::Any,
            },
            Path::from_slices(path_components).unwrap(),
        ));
        assert_eq!(overlaps, expected);
    }

    /// Asserts that `my_registry` reports the correct overlaps after calling my_registry.add_their_incoming_binding for both the `(hash, true)` pair sent by the "other peer" (`fst_expected`), and for the `(hash, false)` pair it sends if any (`snd_expected`, ignored when no such pair is sent) when it adds the given private interest.
    fn assert_theirs(
        my_registry: &mut HashRegistry<16, 32, 1024, 1024, 1024, NamespaceId25, SubspaceId25>,
        their_registry: &mut HashRegistry<16, 32, 1024, 1024, 1024, NamespaceId25, SubspaceId25>,
        namespace: &NamespaceId25,
        subspace: Option<&SubspaceId25>,
        path_components: &[&'static str],
        fst_expected: Vec<Overlap>,
        snd_expected: Vec<Overlap>,
    ) {
        let (fst_hash, snd_hash, _) =
            their_registry.submit_private_interest(&PrivateInterest::new(
                namespace.clone(),
                match subspace {
                    Some(id) => AreaSubspace::Id(id.clone()),
                    None => AreaSubspace::Any,
                },
                Path::from_slices(path_components).unwrap(),
            ));

        let fst_overlaps = my_registry.add_their_incoming_binding(fst_hash, true);
        assert_eq!(
            fst_overlaps, fst_expected,
            "fst_hash: {:?}\n{:#?}",
            fst_hash, my_registry
        );

        if let Some(snd) = snd_hash {
            let snd_overlaps = my_registry.add_their_incoming_binding(snd, false);
            assert_eq!(snd_overlaps, snd_expected);
        }
    }

    fn h(
        p: &PrivateInterest<1024, 1024, 1024, NamespaceId25, SubspaceId25>,
        salt: &[u8; 16],
    ) -> [u8; 32] {
        let mut s = DefaultHasher::new();
        let mut ret = [0; 32];

        for i in 0..4 {
            salt.hash(&mut s);
            p.namespace_id().hash(&mut s);
            p.subspace_id().hash(&mut s);
            p.path().hash(&mut s);
            let digest = s.finish();
            ret[i * 8..(i + 1) * 8].copy_from_slice(&digest.to_be_bytes()[..]);
        }

        return ret;
    }

    // TODO: spec examples the other way around
    // TODO: multiple overlaps in a single addition
}
