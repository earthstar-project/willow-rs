// Pure logic of exchanging salted hashes based on PrivateInterests, and detecting overlaps thereof. No actual message sending.

use std::collections::HashMap;

use multimap::MultiMap;
use willow_data_model::{NamespaceId, PrivateInterest, SubspaceId};

use crate::data_handles::handle_store::{HandleStore, HashMapHandleStore};

/// Tell this struct about your own PrivateInterests and about the salted hashes you receive from the other peer, and this struct tells you which messages to send and where there are overlaps. How nice of it. Does not perform any IO, only implements the logic of the core layer of pio.
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
    pub(crate) my_salt: [u8; SALT_LENGTH],
    pub(crate) h:
        fn(&PrivateInterest<MCL, MCC, MPL, N, S>, &[u8; SALT_LENGTH]) -> [u8; INTEREST_HASH_LENGTH],
    my_handles: HashMapHandleStore<MyHandleInfo<MCL, MCC, MPL, N, S>>,
    my_local_hashes: MultiMap<[u8; INTEREST_HASH_LENGTH], LocalHashInfo>,
    their_handles: HashMapHandleStore<([u8; INTEREST_HASH_LENGTH], bool)>,
    their_hashes: MultiMap<
        [u8; INTEREST_HASH_LENGTH],
        (
            u64,  /* handle in self.their_handles */
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
            their_hashes: MultiMap::new(),
            their_handles: HashMapHandleStore::new(u64::MAX),
        }
    }

    pub(crate) fn their_salt(&self) -> [u8; SALT_LENGTH] {
        let mut salt = self.my_salt.clone();
        for byte in salt.iter_mut() {
            *byte = *byte ^ 255;
        }
        return salt;
    }

    /// Call this when you want to add a PrivateInterest. Returns the one or two salted hashes to send to the peer, and the associated handles.The first hash must be sent with associated bool `true`, the second hash (if there is one) must be sent with associated bool `false`. Also returns any detected matches.
    pub(crate) fn submit_private_interest(
        &mut self,
        p: &PrivateInterest<MCL, MCC, MPL, N, S>,
    ) -> PioBindHashInformation<INTEREST_HASH_LENGTH> {
        // The two hashes to send to the peer (first has associated bool true, the second, optional one has associated bool false).
        let fst = (self.h)(p, &self.my_salt);
        let snd = if p.subspace_id().is_any() {
            None
        } else {
            Some((self.h)(&p.relax(), &self.my_salt))
        };

        // Add handles to handle store for the one or two hash(es) we send.
        let fst_handle = self
            .my_handles
            .bind(MyHandleInfo::Interesting(MyInterestingHandleInfo {
                private_interest: p.clone(),
            }))
            .unwrap();

        let snd_handle = if let Some(_) = snd {
            Some(
                self.my_handles
                    .bind(MyHandleInfo::Uninteresting {
                        interesting_handle: fst_handle,
                    })
                    .unwrap(),
            )
        } else {
            None
        };

        return ((fst, fst_handle), snd.map(|snd| (snd, snd_handle.unwrap())));
    }

    /// Call this after sending both PioBindHash messages (only one if only a single hash and handle was returned by `submit_private_interest`).
    ///
    /// Returns any detected matches.
    pub fn sent_pio_bind_hash_msgs(&mut self, fst_handle: u64) -> Vec<Overlap> {
        let mut overlaps = vec![];

        let p = match self.my_handles.get(fst_handle).unwrap().unwrap() {
            MyHandleInfo::Interesting(MyInterestingHandleInfo { private_interest }) => {
                private_interest.clone()
            }
            _ => unreachable!(),
        };

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
                    interesting_handle: fst_handle,
                },
            );

            if let Some(their_stuff) = self.their_hashes.get_vec(&fst_hash) {
                for (their_handle, they_are_actually_interested) in their_stuff {
                    overlaps.push(Overlap {
                        should_request_capability: !is_original,
                        should_send_capability: is_original,
                        awkward: !they_are_actually_interested && !is_original,
                        my_interesting_handle: fst_handle,
                        their_handle: *their_handle,
                    });
                }
            }

            if !p.subspace_id().is_any() {
                let relaxation = prefix_interest.relax();
                let snd_hash = (self.h)(&relaxation, &self.their_salt());

                self.my_local_hashes.insert(
                    snd_hash,
                    LocalHashInfo {
                        not_a_relaxation: false,
                        has_original_path: is_original,
                        interesting_handle: fst_handle,
                    },
                );

                if let Some(their_stuff) = self.their_hashes.get_vec(&snd_hash) {
                    for (their_handle, they_are_actually_interested) in their_stuff {
                        if *they_are_actually_interested {
                            overlaps.push(Overlap {
                                should_request_capability: !is_original,
                                should_send_capability: false,
                                awkward: false,
                                my_interesting_handle: fst_handle,
                                their_handle: *their_handle,
                            });
                        }
                    }
                }
            }
        }

        overlaps
    }

    /// Call this whenever we received a PioBindHash message. Returns any detected matches.
    pub fn received_pio_bind_hash_msg(
        &mut self,
        hash: [u8; INTEREST_HASH_LENGTH],
        actually_interested: bool,
    ) -> Vec<Overlap> {
        let mut overlaps = vec![];

        let their_handle = self
            .their_handles
            .bind((hash, actually_interested))
            .unwrap();
        self.their_hashes
            .insert(hash, (their_handle, actually_interested));

        if let Some(matches) = self.my_local_hashes.get_vec(&hash) {
            for info in matches.iter() {
                // Overlap does not count if neither peer is actually interested.
                if info.not_a_relaxation || actually_interested {
                    overlaps.push(Overlap {
                        should_request_capability: !info.has_original_path,
                        should_send_capability: info.has_original_path && info.not_a_relaxation,
                        awkward: !actually_interested && !info.has_original_path,
                        my_interesting_handle: info.interesting_handle,
                        their_handle,
                    });
                }
            }
        }

        return overlaps;
    }

    /// Only call this when we know we have bound info, this unwraps the retrieved value.
    pub(crate) fn get_interesting_handle_info(
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

    /// The returned boolean is `true` iff this was an interesting handle. Whole thing returns None if the argument handle is not one we have bound.
    pub(crate) fn try_get_our_handle_info(
        &self,
        my_handle: u64,
    ) -> Option<(&MyInterestingHandleInfo<MCL, MCC, MPL, N, S>, bool)> {
        let my_handle_info = self.my_handles.get(my_handle).unwrap()?;

        match my_handle_info {
            MyHandleInfo::Uninteresting { interesting_handle } => {
                let my_interesting_handle_info =
                    self.my_handles.get(*interesting_handle).unwrap()?;

                match my_interesting_handle_info {
                    MyHandleInfo::Uninteresting { .. } => unreachable!(),
                    MyHandleInfo::Interesting(my_interesting_handle_info) => {
                        return Some((my_interesting_handle_info, false))
                    }
                }
            }
            MyHandleInfo::Interesting(interesting_info) => return Some((interesting_info, true)),
        }
    }

    pub(crate) fn get_their_handle(
        &self,
        their_handle: u64,
    ) -> Option<&([u8; INTEREST_HASH_LENGTH], bool)> {
        self.their_handles.get(their_handle).unwrap()
    }
}

/// Information about which PioBindHash messages we must send.
pub(crate) type PioBindHashInformation<const INTEREST_HASH_LENGTH: usize> = (
    ([u8; INTEREST_HASH_LENGTH], u64),
    Option<([u8; INTEREST_HASH_LENGTH], u64)>,
);

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
    /// The private interest which resulted in this primary handle.
    pub private_interest: PrivateInterest<MCL, MCC, MPL, N, S>,
}

/// Information to report when an overlap has been detected.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub(crate) struct Overlap {
    pub should_request_capability: bool,
    pub should_send_capability: bool,
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
                should_request_capability: false,
                should_send_capability: true,
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
                should_request_capability: true,
                should_send_capability: false,
                awkward: true,
                my_interesting_handle: 6,
                their_handle: 9,
            }],
        );

        // me: gemma, l   they: gemma, m
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["l"],
            vec![],
        );
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
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["n"],
            vec![],
        );
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
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["p"],
            vec![],
        );
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
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["r"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&other_subspace),
            &["r", "s"],
            vec![],
            vec![],
        );

        // me: any, t   they: gemma, t
        assert_mine(&mut my_registry, &namespace, None, &["t"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["t"],
            vec![],
            vec![Overlap {
                should_request_capability: false,
                should_send_capability: true,
                awkward: false,
                my_interesting_handle: 15,
                their_handle: 19,
            }],
        );
    }

    // In this test, the right peer sends its messages first.
    #[test]
    fn spec_examples_reversed() {
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

        // they: gemma, a   me: gemma, a
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![Overlap {
                should_request_capability: false,
                should_send_capability: true,
                awkward: false,
                my_interesting_handle: 0,
                their_handle: 0,
            }],
        );

        // they: any, c   me: any, b
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["c"],
            vec![],
            vec![],
        );
        assert_mine(&mut my_registry, &namespace, None, &["b"], vec![]);

        // they: any, d/e   me: any, d
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["d", "e"],
            vec![],
            vec![],
        );
        assert_mine(&mut my_registry, &namespace, None, &["d"], vec![]);

        // they: gemma, g   me: any, f
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["g"],
            vec![],
            vec![],
        );
        assert_mine(&mut my_registry, &namespace, None, &["f"], vec![]);

        // they: gemma, h/i   me: any, h
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["h", "i"],
            vec![],
            vec![],
        );
        assert_mine(&mut my_registry, &namespace, None, &["h"], vec![]);

        // they: gemma, j   me: any, j/k   awkward!
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["j"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            None,
            &["j", "k"],
            vec![Overlap {
                should_request_capability: true,
                should_send_capability: false,
                awkward: true,
                my_interesting_handle: 6,
                their_handle: 9,
            }],
        );

        // they: gemma, m   me: gemma, l
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["m"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["l"],
            vec![],
        );

        // they: gemma, n/o   me: gemma, n
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["n", "o"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["n"],
            vec![],
        );

        // they: dalton, q   me: gemma, p
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&other_subspace),
            &["q"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["p"],
            vec![],
        );

        // they: dalton, r/s   me: gemma, r
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&other_subspace),
            &["r", "s"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["r"],
            vec![],
        );

        // they: gemma, t   me: any, t
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["t"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            None,
            &["t"],
            vec![Overlap {
                should_request_capability: false,
                should_send_capability: true,
                awkward: false,
                my_interesting_handle: 15,
                their_handle: 19,
            }],
        );
    }

    // The examples from the pio spec, but from the perspective of the right peer!
    #[test]
    fn spec_examples_mirrored() {
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
                should_request_capability: false,
                should_send_capability: true,
                awkward: false,
                my_interesting_handle: 0,
                their_handle: 0,
            }],
            vec![],
        );

        // me: any, c   they: any, b
        assert_mine(&mut my_registry, &namespace, None, &["c"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["b"],
            vec![],
            vec![],
        );

        // me: any, d/e   they: any, d
        assert_mine(&mut my_registry, &namespace, None, &["d", "e"], vec![]);
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["d"],
            vec![Overlap {
                should_request_capability: true,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 3,
                their_handle: 3,
            }],
            vec![],
        );

        // me: gemma, g   they: any, f
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["g"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["f"],
            vec![],
            vec![],
        );

        // me: gemma, h/i   they: any, h
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["h", "i"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["h"],
            vec![Overlap {
                should_request_capability: true,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 6,
                their_handle: 5,
            }],
            vec![],
        );

        // me: gemma, j   they: any, j/k   awkward!
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["j"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["j", "k"],
            vec![],
            vec![],
        );

        // me: gemma, m   they: gemma, l
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["m"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["l"],
            vec![],
            vec![],
        );

        // me: gemma, n/o   they: gemma, n
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["n", "o"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["n"],
            vec![Overlap {
                should_request_capability: true,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 12,
                their_handle: 9,
            }],
            vec![],
        );

        // me: dalton, q   they: gemma, p
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&other_subspace),
            &["q"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["p"],
            vec![],
            vec![],
        );

        // me: dalton, r/s  they:  gemma, r
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&other_subspace),
            &["r", "s"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["r"],
            vec![],
            vec![],
        );

        // me: gemma, t   they: any, t
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["t"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["t"],
            vec![Overlap {
                should_request_capability: false,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 18,
                their_handle: 15,
            }],
            vec![],
        );
    }

    // The examples from the pio spec, but from the perspective of the right peer, and with the right peer sending first.
    #[test]
    fn spec_examples_mirrored_reversed() {
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

        // they: gemma, a   me: gemma, a
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![Overlap {
                should_request_capability: false,
                should_send_capability: true,
                awkward: false,
                my_interesting_handle: 0,
                their_handle: 0,
            }],
        );

        // they: any, b   me: any, c
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["b"],
            vec![],
            vec![],
        );
        assert_mine(&mut my_registry, &namespace, None, &["c"], vec![]);

        // they: any, d   me: any, d/e
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["d"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            None,
            &["d", "e"],
            vec![Overlap {
                should_request_capability: true,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 3,
                their_handle: 3,
            }],
        );

        // they: any, f   me: gemma, g
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["f"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["g"],
            vec![],
        );

        // they: any, h   me: gemma, h/i
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["h"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["h", "i"],
            vec![Overlap {
                should_request_capability: true,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 6,
                their_handle: 5,
            }],
        );

        // they: any, j/k   me: gemma, j   awkward!
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["j", "k"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["j"],
            vec![],
        );

        // they: gemma, l   me: gemma, m
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["l"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["m"],
            vec![],
        );

        // they: gemma, n   me: gemma, n/o
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["n"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["n", "o"],
            vec![Overlap {
                should_request_capability: true,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 12,
                their_handle: 9,
            }],
        );

        // they: gemma, p   me: dalton, q
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["p"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&other_subspace),
            &["q"],
            vec![],
        );

        // hey:  gemma, r   me: dalton, r/s
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["r"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&other_subspace),
            &["r", "s"],
            vec![],
        );

        // they: any, t   me: gemma, t
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["t"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["t"],
            vec![Overlap {
                should_request_capability: false,
                should_send_capability: false,
                awkward: false,
                my_interesting_handle: 18,
                their_handle: 15,
            }],
        );
    }

    // Things work when a single private interest triggers multiple overlaps.
    #[test]
    fn multiple_matches() {
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

        // me: gemma, a; me: dalton, a;   they: any, a
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            Some(&other_subspace),
            &["a"],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            None,
            &["a"],
            vec![
                Overlap {
                    should_request_capability: false,
                    should_send_capability: false,
                    awkward: false,
                    my_interesting_handle: 0,
                    their_handle: 0,
                },
                Overlap {
                    should_request_capability: false,
                    should_send_capability: false,
                    awkward: false,
                    my_interesting_handle: 2,
                    their_handle: 0,
                },
            ],
            vec![],
        );
    }

    #[test]
    fn multiple_matches_reversed() {
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

        // they: gemma, a; dalton; a   me: any, a
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&subspace),
            &["a"],
            vec![],
            vec![],
        );
        assert_theirs(
            &mut my_registry,
            &mut their_registry,
            &namespace,
            Some(&other_subspace),
            &["a"],
            vec![],
            vec![],
        );
        assert_mine(
            &mut my_registry,
            &namespace,
            None,
            &["a"],
            vec![
                Overlap {
                    should_request_capability: false,
                    should_send_capability: true,
                    awkward: false,
                    my_interesting_handle: 0,
                    their_handle: 1,
                },
                Overlap {
                    should_request_capability: false,
                    should_send_capability: true,
                    awkward: false,
                    my_interesting_handle: 0,
                    their_handle: 3,
                },
            ],
        );
    }

    fn assert_mine(
        registry: &mut HashRegistry<16, 32, 1024, 1024, 1024, NamespaceId25, SubspaceId25>,
        namespace: &NamespaceId25,
        subspace: Option<&SubspaceId25>,
        path_components: &[&'static str],
        expected: Vec<Overlap>,
    ) {
        let (fst, _snd) = registry.submit_private_interest(&PrivateInterest::new(
            namespace.clone(),
            match subspace {
                Some(id) => AreaSubspace::Id(id.clone()),
                None => AreaSubspace::Any,
            },
            Path::from_slices(path_components).unwrap(),
        ));
        let overlaps = registry.sent_pio_bind_hash_msgs(fst.1);
        assert_eq!(overlaps, expected, "registry: {:?}", registry);
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
        let (fst, snd) = their_registry.submit_private_interest(&PrivateInterest::new(
            namespace.clone(),
            match subspace {
                Some(id) => AreaSubspace::Id(id.clone()),
                None => AreaSubspace::Any,
            },
            Path::from_slices(path_components).unwrap(),
        ));

        let fst_overlaps = my_registry.received_pio_bind_hash_msg(fst.0, true);
        // println!("fst: {:?}, {:?}, {:?}", namespace, subspace, path_components);
        assert_eq!(
            fst_overlaps, fst_expected,
            "fst_hash: {:?}\n{:#?}",
            fst.0, my_registry
        );

        if let Some((snd, _)) = snd {
            // println!("snd: {:?}, {:?}, {:?}", namespace, subspace, path_components);
            let snd_overlaps = my_registry.received_pio_bind_hash_msg(snd, false);
            assert_eq!(
                snd_overlaps, snd_expected,
                "snd_hash: {:?}\n{:#?}",
                snd, my_registry
            );
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
}
