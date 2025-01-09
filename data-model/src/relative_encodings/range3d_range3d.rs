use crate::{
    grouping::{Range, Range3d, RangeEnd},
    Path, SubspaceId,
};

use compact_u64::{CompactU64, Tag, TagWidth};
use ufotofu::{BulkConsumer, BulkProducer};

use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodableSync, DecodeError, Encodable, EncodableKnownSize,
    EncodableSync, RelativeDecodable, RelativeDecodableCanonic, RelativeDecodableSync,
    RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};
use willow_encoding::is_bitflagged;

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodable<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + Encodable + std::fmt::Debug,
{
    /// Encodes this [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let start_to_start = self.times().start.abs_diff(r.times().start);
        let start_to_end = match r.times().end {
            RangeEnd::Closed(end) => self.times().start.abs_diff(end),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_start = match self.times().end {
            RangeEnd::Closed(end) => end.abs_diff(r.times().start),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_end = match (&self.times().end, &r.times().end) {
            (RangeEnd::Closed(self_end), RangeEnd::Closed(ref_end)) => self_end.abs_diff(*ref_end),
            (RangeEnd::Closed(_), RangeEnd::Open) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Closed(_)) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Open) => 0, // shouldn't matter right???
        };

        let start_time_diff = core::cmp::min(start_to_start, start_to_end);

        let end_time_diff = core::cmp::min(end_to_start, end_to_end);

        let mut header_1 = 0b0000_0000;

        // Bits 0, 1 - Encode r.get_subspaces().start?
        if self.subspaces().start == r.subspaces().start {
            header_1 |= 0b0100_0000;
        } else if r.subspaces().end == self.subspaces().start {
            header_1 |= 0b1000_0000;
        } else {
            header_1 |= 0b1100_0000;
        }

        // Bits 2, 3 - Encode r.get_subspaces().end?
        if self.subspaces().end == RangeEnd::Open {
            // Do nothing
        } else if self.subspaces().end == r.subspaces().start {
            header_1 |= 0b0001_0000;
        } else if self.subspaces().end == r.subspaces().end {
            header_1 |= 0b0010_0000;
        } else if self.subspaces().end != RangeEnd::Open {
            header_1 |= 0b0011_0000;
        }

        // Bit 4 - Encode r.get_paths().start relative to ref.get_paths().start or to ref.get_paths().end?
        if let RangeEnd::Closed(ref_path_end) = &r.paths().end {
            let lcp_start_start = self.paths().start.longest_common_prefix(&r.paths().start);
            let lcp_start_end = self.paths().start.longest_common_prefix(ref_path_end);

            if lcp_start_start.component_count() >= lcp_start_end.component_count() {
                header_1 |= 0b0000_1000;
            }
        } else {
            header_1 |= 0b0000_1000;
        }

        // Bit 5 - Self path end open?
        if self.paths().end == RangeEnd::Open {
            header_1 |= 0b0000_0100;
        }

        // Bit 6 - Encode r.get_paths().end relative to ref.get_paths().start or to ref.get_paths().end (if at all)?
        match (&self.paths().end, &r.paths().end) {
            (RangeEnd::Closed(self_path_end), RangeEnd::Closed(ref_path_end)) => {
                let lcp_end_start = self_path_end.longest_common_prefix(&r.paths().start);
                let lcp_end_end = self_path_end.longest_common_prefix(ref_path_end);

                if lcp_end_start.component_count() >= lcp_end_end.component_count() {
                    header_1 |= 0b0000_0010;
                }
            }
            (RangeEnd::Closed(_), RangeEnd::Open) => {
                header_1 |= 0b0000_0010;
            }
            (RangeEnd::Open, RangeEnd::Closed(_)) => {}
            (RangeEnd::Open, RangeEnd::Open) => {}
        }

        // Bit 7 - Self time end open?
        if self.times().end == RangeEnd::Open {
            header_1 |= 0b0000_0001;
        }

        consumer.consume(header_1).await?;

        let mut header_2 = 0b0000_0000;

        // Bit 8 - Encode r.get_times().start relative to ref.get_times().start or ref.get_times().end?
        if start_to_start <= start_to_end {
            header_2 |= 0b1000_0000;
        }

        // Bit 9 -Add or subtract start_time_diff?
        if is_bitflagged(header_2, 0) && self.times().start >= r.times().start
            || !is_bitflagged(header_2, 0) && self.times().start >= r.times().end
        {
            header_2 |= 0b0100_0000;
        }

        // Bit 10, 11 - 2-bit integer n such that 2^n gives compact_width(start_time_diff)
        let start_diff_tag = Tag::min_tag(start_time_diff, TagWidth::two());
        header_2 |= start_diff_tag.data_at_offset(2);

        // Bit 12 - Encode r.get_times().end relative to ref.get_times().start or ref.get_times().end (if at all)?
        if self.times().end != RangeEnd::Open && end_to_start <= end_to_end {
            header_2 |= 0b0000_1000;
        }

        // Bit 13 - Add or subtract end_time_diff (if encoding it at all)?
        if self.times().end == RangeEnd::Open {
            // do nothing
        } else if (is_bitflagged(header_2, 4) && self.times().end >= r.times().start)
            || (!is_bitflagged(header_2, 4) && self.times().end >= r.times().end)
        {
            header_2 |= 0b0000_0100;
        }

        // Bits 14, 15 - ignored, or 2-bit integer n such that 2^n gives compact_width(end_time_diff)
        if self.times().end == RangeEnd::Open {
            // do nothing
        } else {
            let end_diff_tag = Tag::min_tag(end_time_diff, TagWidth::two());
            header_2 |= end_diff_tag.data_at_offset(6);
        }

        consumer.consume(header_2).await?;

        if (self.subspaces().start == r.subspaces().start)
            || (r.subspaces().end == self.subspaces().start)
        {
            // Don't encode
        } else {
            self.subspaces().start.encode(consumer).await?;
        }

        if self.subspaces().end == RangeEnd::Open
            || (self.subspaces().end == r.subspaces().start)
            || (self.subspaces().end == r.subspaces().end)
        {
            // Don't encode end subspace
        } else if let RangeEnd::Closed(end_subspace) = &self.subspaces().end {
            end_subspace.encode(consumer).await?;
        }

        if is_bitflagged(header_1, 4) {
            self.paths()
                .start
                .relative_encode(consumer, &r.paths().start)
                .await?;
        } else if let RangeEnd::Closed(end_path) = &r.paths().end {
            self.paths()
                .start
                .relative_encode(consumer, end_path)
                .await?;
        }

        if let RangeEnd::Closed(end_path) = &self.paths().end {
            if is_bitflagged(header_1, 6) {
                end_path.relative_encode(consumer, &r.paths().start).await?
            } else if let RangeEnd::Closed(ref_end_path) = &r.paths().end {
                end_path.relative_encode(consumer, ref_end_path).await?;
            }
        }

        CompactU64(start_time_diff)
            .relative_encode(consumer, &start_diff_tag.encoding_width())
            .await?;

        if self.times().end != RangeEnd::Open {
            let end_diff_tag = Tag::min_tag(end_time_diff, TagWidth::two());

            CompactU64(end_time_diff)
                .relative_encode(consumer, &end_diff_tag.encoding_width())
                .await?;
        }

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodable<Range3d<MCL, MCC, MPL, S>, Blame> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
    /// Decodes a [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
    ///
    /// Will return an error if the encoding has not been produced by the corresponding encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
          let header_1 = produce_byte(producer).await?;

          let subspace_start_flags = header_1 & 0b1100_0000;
          let subspace_end_flags = header_1 & 0b0011_0000;
          let is_path_start_rel_to_start = is_bitflagged(header_1, 4);
          let is_path_end_open = is_bitflagged(header_1, 5);
          let is_path_end_rel_to_start = is_bitflagged(header_1, 6);
          let is_times_end_open = is_bitflagged(header_1, 7);

          let header_2 = produce_byte(producer).await?;

          let is_time_start_rel_to_start = is_bitflagged(header_2, 0);
          let add_or_subtract_start_time_diff = is_bitflagged(header_2, 1);
          let start_time_diff_compact_width =
              CompactWidth::decode_fixed_width_bitmask(header_2, 2);
          let is_time_end_rel_to_start = is_bitflagged(header_2, 4);
          let add_or_subtract_end_time_diff = is_bitflagged(header_2, 5);
          let end_time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header_2, 6);

          // Decode subspace start
          let subspace_start = match subspace_start_flags {
              0b0100_0000 => reference.subspaces().start.clone(),
              0b1000_0000 => match &reference.subspaces().end {
                  RangeEnd::Closed(end) => end.clone(),
                  RangeEnd::Open => Err(DecodeError::InvalidInput)?,
              },
              0b1100_0000 => S::decode_canonical(producer).await?,
              // This can only be b0000_0000 (which is not valid!)
              _ => Err(DecodeError::InvalidInput)?,
          };

          let subspace_end = match subspace_end_flags {
              0b0000_0000 => RangeEnd::Open,
              0b0001_0000 => RangeEnd::Closed(reference.subspaces().start.clone()),
              0b0010_0000 => match &reference.subspaces().end {
                  RangeEnd::Closed(end) => RangeEnd::Closed(end.clone()),
                  RangeEnd::Open => Err(DecodeError::InvalidInput)?,
              },
              // This can only be 0b0011_0000
              _ => RangeEnd::Closed(S::decode_relation(producer).await?),
          };

          let path_start = match (is_path_start_rel_to_start, &reference.paths().end) {
              (true, RangeEnd::Closed(_)) => {
                  Path::relative_decode_canonical(&reference.paths().start, producer).await?
              }
              (true, RangeEnd::Open) => {
                  Path::relative_decode_canonical(&reference.paths().start, producer).await?
              }
              (false, RangeEnd::Closed(path_end)) => {
                  Path::relative_decode_canonical(path_end, producer).await?
              }
              (false, RangeEnd::Open) => Err(DecodeError::InvalidInput)?,
          };

          let path_end = if is_path_end_open {
              RangeEnd::Open
          } else if is_path_end_rel_to_start {
              RangeEnd::Closed(
                  Path::relative_decode_canonical(&reference.paths().start, producer).await?,
              )
          } else {
              match &reference.paths().end {
                  RangeEnd::Closed(end) => {
                      RangeEnd::Closed(Path::relative_decode_canonical(end, producer).await?)
                  }
                  RangeEnd::Open => Err(DecodeError::InvalidInput)?,
              }
          };

          let start_time_diff =
              decode_compact_width_be_relation(start_time_diff_compact_width, producer).await?;

          let time_start = match (is_time_start_rel_to_start, add_or_subtract_start_time_diff) {
              (true, true) => reference.times().start.checked_add(start_time_diff),
              (true, false) => reference.times().start.checked_sub(start_time_diff),
              (false, true) => match reference.times().end {
                  RangeEnd::Closed(ref_end) => ref_end.checked_add(start_time_diff),
                  RangeEnd::Open => Err(DecodeError::InvalidInput)?,
              },
              (false, false) => match reference.times().end {
                  RangeEnd::Closed(ref_end) => ref_end.checked_sub(start_time_diff),
                  RangeEnd::Open => Err(DecodeError::InvalidInput)?,
              },
          }
          .ok_or(DecodeError::InvalidInput)?;

          let time_end = if is_times_end_open {
              RangeEnd::Open
          } else {
              let end_time_diff =
                  decode_compact_width_be_relation(end_time_diff_compact_width, producer).await?;

              let time_end = match (is_time_end_rel_to_start, add_or_subtract_end_time_diff) {
                  (true, true) => reference
                      .times()
                      .start
                      .checked_add(end_time_diff)
                      .ok_or(DecodeError::InvalidInput)?,

                  (true, false) => reference
                      .times()
                      .start
                      .checked_sub(end_time_diff)
                      .ok_or(DecodeError::InvalidInput)?,

                  (false, true) => match reference.times().end {
                      RangeEnd::Closed(ref_end) => ref_end
                          .checked_add(end_time_diff)
                          .ok_or(DecodeError::InvalidInput)?,

                      RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                  },
                  (false, false) => match reference.times().end {
                      RangeEnd::Closed(ref_end) => ref_end
                          .checked_sub(end_time_diff)
                          .ok_or(DecodeError::InvalidInput)?,

                      RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                  },
              };

              RangeEnd::Closed(time_end)
          };

          Ok(Self::new(
              Range {
                  start: subspace_start,
                  end: subspace_end,
              },
              Range {
                  start: path_start,
                  end: path_end,
              },
              Range {
                  start: time_start,
                  end: time_end,
              },
          ))
        */

        relative_decode_maybe_canonic::<false, MCL, MCC, MPL, S, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodableCanonic<Range3d<MCL, MCC, MPL, S>, Blame, Blame> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<true, MCL, MCC, MPL, S, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableKnownSize<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableKnownSize + std::fmt::Debug,
{
    fn relative_len_of_encoding(&self, r: &Range3d<MCL, MCC, MPL, S>) -> usize {
        let start_to_start = self.times().start.abs_diff(r.times().start);
        let start_to_end = match r.times().end {
            RangeEnd::Closed(end) => self.times().start.abs_diff(end),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_start = match self.times().end {
            RangeEnd::Closed(end) => end.abs_diff(r.times().start),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_end = match (&self.times().end, &r.times().end) {
            (RangeEnd::Closed(self_end), RangeEnd::Closed(ref_end)) => self_end.abs_diff(*ref_end),
            (RangeEnd::Closed(_), RangeEnd::Open) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Closed(_)) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Open) => 0, // shouldn't matter right???
        };

        let start_time_diff = core::cmp::min(start_to_start, start_to_end);

        let end_time_diff = core::cmp::min(end_to_start, end_to_end);

        let subspace_start_len = if (self.subspaces().start == r.subspaces().start)
            || (r.subspaces().end == self.subspaces().start)
        {
            0
        } else {
            self.subspaces().start.len_of_encoding()
        };

        let subspace_end_len = if self.subspaces().end == RangeEnd::Open
            || (self.subspaces().end == r.subspaces().start)
            || (self.subspaces().end == r.subspaces().end)
        {
            // Don't encode end subspace
            0
        } else if let RangeEnd::Closed(end_subspace) = &self.subspaces().end {
            end_subspace.len_of_encoding()
        } else {
            0
        };

        let path_start_rel_to_start = if let RangeEnd::Closed(ref_path_end) = &r.paths().end {
            let lcp_start_start = self.paths().start.longest_common_prefix(&r.paths().start);
            let lcp_start_end = self.paths().start.longest_common_prefix(ref_path_end);

            lcp_start_start.component_count() >= lcp_start_end.component_count()
        } else {
            true
        };

        let path_start_len = if path_start_rel_to_start {
            self.paths()
                .start
                .relative_len_of_encoding(&r.paths().start)
        } else if let RangeEnd::Closed(end_path) = &r.paths().end {
            self.paths().start.relative_len_of_encoding(end_path)
        } else {
            panic!("Tried to encode a path range start relative to an open end")
        };

        let path_end_rel_to_start = match (&self.paths().end, &r.paths().end) {
            (RangeEnd::Closed(self_path_end), RangeEnd::Closed(ref_path_end)) => {
                let lcp_end_start = self_path_end.longest_common_prefix(&r.paths().start);
                let lcp_end_end = self_path_end.longest_common_prefix(ref_path_end);

                lcp_end_start.component_count() >= lcp_end_end.component_count()
            }
            (RangeEnd::Closed(_), RangeEnd::Open) => true,
            (RangeEnd::Open, RangeEnd::Closed(_)) => false,
            (RangeEnd::Open, RangeEnd::Open) => false,
        };

        let path_end_len = if let RangeEnd::Closed(end_path) = &self.paths().end {
            if path_end_rel_to_start {
                end_path.relative_len_of_encoding(&r.paths().start)
            } else if let RangeEnd::Closed(ref_end_path) = &r.paths().end {
                end_path.relative_len_of_encoding(ref_end_path)
            } else {
                0
            }
        } else {
            0
        };

        let start_diff_tag = Tag::min_tag(start_time_diff, TagWidth::two());

        let start_diff_len =
            CompactU64(start_time_diff).relative_len_of_encoding(&start_diff_tag.encoding_width());

        let end_diff_len = if self.times().end != RangeEnd::Open {
            let end_diff_tag = Tag::min_tag(end_time_diff, TagWidth::two());

            CompactU64(end_time_diff).relative_len_of_encoding(&end_diff_tag.encoding_width())
        } else {
            0
        };

        2 + subspace_start_len
            + subspace_end_len
            + path_start_len
            + path_end_len
            + start_diff_len
            + end_diff_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableSync<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableSync + std::fmt::Debug,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodableSync<Range3d<MCL, MCC, MPL, S>, Blame> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
}

async fn relative_decode_maybe_canonic<
    const CANONIC: bool,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S,
    P,
>(
    producer: &mut P,
    r: &Range3d<MCL, MCC, MPL, S>,
) -> Result<Range3d<MCL, MCC, MPL, S>, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
    let header_1 = producer.produce_item().await?;

    let subspace_start_flags = header_1 & 0b1100_0000;
    let subspace_end_flags = header_1 & 0b0011_0000;
    let is_path_start_rel_to_start = is_bitflagged(header_1, 4);
    let is_path_end_open = is_bitflagged(header_1, 5);
    let is_path_end_rel_to_start = is_bitflagged(header_1, 6);
    let is_times_end_open = is_bitflagged(header_1, 7);

    let header_2 = producer.produce_item().await?;

    let is_time_start_rel_to_start = is_bitflagged(header_2, 0);
    let add_or_subtract_start_time_diff = is_bitflagged(header_2, 1);

    let start_time_diff_tag = Tag::from_raw(header_2, TagWidth::two(), 2);
    let is_time_end_rel_to_start = is_bitflagged(header_2, 4);
    let add_or_subtract_end_time_diff = is_bitflagged(header_2, 5);
    let end_time_diff_tag = Tag::from_raw(header_2, TagWidth::two(), 6);

    // Decode subspace start
    let subspace_start = match subspace_start_flags {
        0b0100_0000 => r.subspaces().start.clone(),
        0b1000_0000 => match &r.subspaces().end {
            RangeEnd::Closed(end) => end.clone(),
            RangeEnd::Open => Err(DecodeError::Other(Blame::TheirFault))?,
        },
        0b1100_0000 => {
            let decoded_subspace = if CANONIC {
                S::decode_canonic(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?
            } else {
                S::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?
            };

            if decoded_subspace == r.subspaces().start || r.subspaces().end == decoded_subspace {
                return Err(DecodeError::Other(Blame::TheirFault));
            }

            decoded_subspace
        }
        // This can only be b0000_0000 (which is not valid!)
        _ => Err(DecodeError::Other(Blame::TheirFault))?,
    };

    let subspace_end = match subspace_end_flags {
        0b0000_0000 => RangeEnd::Open,
        0b0001_0000 => RangeEnd::Closed(r.subspaces().start.clone()),
        0b0010_0000 => match &r.subspaces().end {
            RangeEnd::Closed(end) => RangeEnd::Closed(end.clone()),
            RangeEnd::Open => Err(DecodeError::Other(Blame::TheirFault))?,
        },
        // This can only be 0b0011_0000
        _ => {
            let decoded_subspace = if CANONIC {
                RangeEnd::Closed(
                    S::decode_canonic(producer)
                        .await
                        .map_err(DecodeError::map_other_from)?,
                )
            } else {
                RangeEnd::Closed(
                    S::decode(producer)
                        .await
                        .map_err(DecodeError::map_other_from)?,
                )
            };

            if decoded_subspace == r.subspaces().start || r.subspaces().end == decoded_subspace {
                return Err(DecodeError::Other(Blame::TheirFault));
            }

            decoded_subspace
        }
    };

    // Check subspace end...

    let path_start = match (is_path_start_rel_to_start, &r.paths().end) {
        (true, RangeEnd::Closed(_)) => {
            if CANONIC {
                Path::relative_decode_canonic(producer, &r.paths().start).await?
            } else {
                Path::relative_decode(producer, &r.paths().start).await?
            }
        }
        (true, RangeEnd::Open) => {
            if CANONIC {
                Path::relative_decode_canonic(producer, &r.paths().start).await?
            } else {
                Path::relative_decode(producer, &r.paths().start).await?
            }
        }
        (false, RangeEnd::Closed(path_end)) => {
            if CANONIC {
                Path::relative_decode_canonic(producer, path_end).await?
            } else {
                Path::relative_decode(producer, path_end).await?
            }
        }
        (false, RangeEnd::Open) => Err(DecodeError::Other(Blame::TheirFault))?,
    };

    // Canonicity check for path start
    if CANONIC {
        match &r.paths().end {
            RangeEnd::Closed(ref_path_end) => {
                let lcp_start_start = path_start.longest_common_prefix(&r.paths().start);
                let lcp_start_end = path_start.longest_common_prefix(ref_path_end);

                let expected_is_start_rel_to_start =
                    lcp_start_start.component_count() >= lcp_start_end.component_count();

                if expected_is_start_rel_to_start != is_path_start_rel_to_start {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }
            }
            RangeEnd::Open => {
                if !is_path_start_rel_to_start {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }
            }
        }
    }
    // Canonicity check for path start over

    let path_end = if is_path_end_open {
        RangeEnd::Open
    } else if is_path_end_rel_to_start {
        if CANONIC {
            RangeEnd::Closed(Path::relative_decode_canonic(producer, &r.paths().start).await?)
        } else {
            RangeEnd::Closed(Path::relative_decode(producer, &r.paths().start).await?)
        }
    } else {
        match &r.paths().end {
            RangeEnd::Closed(end) => {
                if CANONIC {
                    RangeEnd::Closed(Path::relative_decode_canonic(producer, end).await?)
                } else {
                    RangeEnd::Closed(Path::relative_decode(producer, end).await?)
                }
            }
            RangeEnd::Open => Err(DecodeError::Other(Blame::TheirFault))?,
        }
    };

    // Canonicity check for path end
    if CANONIC {
        match &path_end {
            RangeEnd::Closed(p_end) => match &r.paths().end {
                RangeEnd::Closed(ref_end) => {
                    let lcp_end_start = p_end.longest_common_prefix(&r.paths().start);
                    let lcp_end_end = p_end.longest_common_prefix(ref_end);

                    let expected_is_path_end_rel_to_start =
                        lcp_end_start.component_count() >= lcp_end_end.component_count();

                    if expected_is_path_end_rel_to_start != is_path_end_rel_to_start {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }
                RangeEnd::Open => {}
            },
            RangeEnd::Open => {
                if is_path_end_rel_to_start {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }
            }
        }
    }
    // End canonicity check

    let start_time_diff = if CANONIC {
        CompactU64::relative_decode_canonic(producer, &start_time_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    } else {
        CompactU64::relative_decode(producer, &start_time_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    };

    let time_start = match (is_time_start_rel_to_start, add_or_subtract_start_time_diff) {
        (true, true) => r.times().start.checked_add(start_time_diff),
        (true, false) => r.times().start.checked_sub(start_time_diff),
        (false, true) => match r.times().end {
            RangeEnd::Closed(ref_end) => ref_end.checked_add(start_time_diff),
            RangeEnd::Open => Err(DecodeError::Other(Blame::TheirFault))?,
        },
        (false, false) => match r.times().end {
            RangeEnd::Closed(ref_end) => ref_end.checked_sub(start_time_diff),
            RangeEnd::Open => Err(DecodeError::Other(Blame::TheirFault))?,
        },
    }
    .ok_or(DecodeError::Other(Blame::TheirFault))?;

    // Canonicity check for start time
    if CANONIC {
        match r.times().end {
            RangeEnd::Closed(ref_time_end) => {
                let start_to_start = time_start.abs_diff(r.times().start);
                let start_to_end = time_start.abs_diff(ref_time_end);

                let expected_is_start_rel_to_start = start_to_start <= start_to_end;

                if expected_is_start_rel_to_start != is_time_start_rel_to_start {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }

                let expected_add_or_subtract_start_time_diff = is_time_start_rel_to_start
                    && time_start >= r.times().start
                    || !expected_is_start_rel_to_start && time_start >= ref_time_end;

                if expected_add_or_subtract_start_time_diff != add_or_subtract_start_time_diff {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }
            }
            RangeEnd::Open => {
                if !is_time_start_rel_to_start {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }

                // I need to very add or subtract time diff here.
                let expected_add_or_subtract_time_diff = time_start >= r.times().start;

                if expected_add_or_subtract_time_diff != add_or_subtract_start_time_diff {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }
            }
        }
    }
    // End of canonicity check for start time

    let time_end = if is_times_end_open {
        if CANONIC {
            if add_or_subtract_end_time_diff {
                return Err(DecodeError::Other(Blame::TheirFault));
            }

            if is_time_end_rel_to_start {
                return Err(DecodeError::Other(Blame::TheirFault));
            }

            let end_time_diff_compact_width_flags = 0b0000_0011;
            if header_2 & end_time_diff_compact_width_flags != 0b0000_0000 {
                return Err(DecodeError::Other(Blame::TheirFault));
            }
        }

        RangeEnd::Open
    } else {
        let end_time_diff = if CANONIC {
            CompactU64::relative_decode_canonic(producer, &end_time_diff_tag)
                .await
                .map_err(DecodeError::map_other_from)?
                .0
        } else {
            CompactU64::relative_decode(producer, &end_time_diff_tag)
                .await
                .map_err(DecodeError::map_other_from)?
                .0
        };

        let time_end = match (is_time_end_rel_to_start, add_or_subtract_end_time_diff) {
            (true, true) => r
                .times()
                .start
                .checked_add(end_time_diff)
                .ok_or(DecodeError::Other(Blame::TheirFault))?,

            (true, false) => r
                .times()
                .start
                .checked_sub(end_time_diff)
                .ok_or(DecodeError::Other(Blame::TheirFault))?,

            (false, true) => match r.times().end {
                RangeEnd::Closed(ref_end) => ref_end
                    .checked_add(end_time_diff)
                    .ok_or(DecodeError::Other(Blame::TheirFault))?,

                RangeEnd::Open => Err(DecodeError::Other(Blame::TheirFault))?,
            },
            (false, false) => match r.times().end {
                RangeEnd::Closed(ref_end) => ref_end
                    .checked_sub(end_time_diff)
                    .ok_or(DecodeError::Other(Blame::TheirFault))?,

                RangeEnd::Open => Err(DecodeError::Other(Blame::TheirFault))?,
            },
        };

        let end_to_start = time_end.abs_diff(r.times().start);
        let end_to_end = match &r.times().end {
            RangeEnd::Closed(ref_end) => time_end.abs_diff(*ref_end),
            RangeEnd::Open => u64::MAX,
        };

        if CANONIC {
            let expected_is_time_end_rel_to_start = end_to_start <= end_to_end;
            if expected_is_time_end_rel_to_start != is_time_end_rel_to_start {
                return Err(DecodeError::Other(Blame::TheirFault));
            }

            let expected_end_time_diff = core::cmp::min(end_to_start, end_to_end);

            if expected_end_time_diff != end_time_diff {
                return Err(DecodeError::Other(Blame::TheirFault));
            }

            let expected_add_or_subtract_end_time_diff = (is_time_end_rel_to_start
                && time_end >= r.times().start)
                || (!is_time_end_rel_to_start && time_end >= r.times().end);

            if expected_add_or_subtract_end_time_diff != add_or_subtract_end_time_diff {
                return Err(DecodeError::Other(Blame::TheirFault));
            }
        }

        RangeEnd::Closed(time_end)
    };

    Ok(Range3d::new(
        Range {
            start: subspace_start,
            end: subspace_end,
        },
        Range {
            start: path_start,
            end: path_end,
        },
        Range {
            start: time_start,
            end: time_end,
        },
    ))
}
