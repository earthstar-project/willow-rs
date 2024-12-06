use crate::{grouping::Range3d, SubspaceId};

use ufotofu::{BulkConsumer, BulkProducer};

use ufotofu_codec::{
    Decodable, DecodableCanonic, DecodableSync, DecodeError, DecodingWentWrong, Encodable,
    EncodableKnownSize, EncodableSync, RelativeDecodable, RelativeDecodableCanonic,
    RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};

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
        /*
        let start_to_start = self.times().start.abs_diff(reference.times().start);
        let start_to_end = match reference.times().end {
            RangeEnd::Closed(end) => self.times().start.abs_diff(end),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_start = match self.times().end {
            RangeEnd::Closed(end) => end.abs_diff(reference.times().start),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_end = match (&self.times().end, &reference.times().end) {
            (RangeEnd::Closed(self_end), RangeEnd::Closed(ref_end)) => {
                self_end.abs_diff(*ref_end)
            }
            (RangeEnd::Closed(_), RangeEnd::Open) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Closed(_)) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Open) => 0, // shouldn't matter right???
        };

        let start_time_diff = core::cmp::min(start_to_start, start_to_end);

        let end_time_diff = core::cmp::min(end_to_start, end_to_end);

        let mut header_1 = 0b0000_0000;

        // Bits 0, 1 - Encode r.get_subspaces().start?
        if self.subspaces().start == reference.subspaces().start {
            header_1 |= 0b0100_0000;
        } else if reference.subspaces().end == self.subspaces().start {
            header_1 |= 0b1000_0000;
        } else {
            header_1 |= 0b1100_0000;
        }

        // Bits 2, 3 - Encode r.get_subspaces().end?
        if self.subspaces().end == RangeEnd::Open {
            // Do nothing
        } else if self.subspaces().end == reference.subspaces().start {
            header_1 |= 0b0001_0000;
        } else if self.subspaces().end == reference.subspaces().end {
            header_1 |= 0b0010_0000;
        } else if self.subspaces().end != RangeEnd::Open {
            header_1 |= 0b0011_0000;
        }

        // Bit 4 - Encode r.get_paths().start relative to ref.get_paths().start or to ref.get_paths().end?
        if let RangeEnd::Closed(ref_path_end) = &reference.paths().end {
            let lcp_start_start = self
                .paths()
                .start
                .longest_common_prefix(&reference.paths().start);
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
        match (&self.paths().end, &reference.paths().end) {
            (RangeEnd::Closed(self_path_end), RangeEnd::Closed(ref_path_end)) => {
                let lcp_end_start =
                    self_path_end.longest_common_prefix(&reference.paths().start);
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
        if is_bitflagged(header_2, 0) && self.times().start >= reference.times().start
            || !is_bitflagged(header_2, 0) && self.times().start >= reference.times().end
        {
            header_2 |= 0b0100_0000;
        }

        // Bit 10, 11 - 2-bit integer n such that 2^n gives compact_width(start_time_diff)
        header_2 |= CompactWidth::from_u64(start_time_diff).bitmask(2);

        // Bit 12 - Encode r.get_times().end relative to ref.get_times().start or ref.get_times().end (if at all)?
        if self.times().end != RangeEnd::Open && end_to_start <= end_to_end {
            header_2 |= 0b0000_1000;
        }

        // Bit 13 - Add or subtract end_time_diff (if encoding it at all)?
        if self.times().end == RangeEnd::Open {
            // do nothing
        } else if (is_bitflagged(header_2, 4) && self.times().end >= reference.times().start)
            || (!is_bitflagged(header_2, 4) && self.times().end >= reference.times().end)
        {
            header_2 |= 0b0000_0100;
        }

        // Bits 14, 15 - ignored, or 2-bit integer n such that 2^n gives compact_width(end_time_diff)
        if self.times().end == RangeEnd::Open {
            // do nothing
        } else {
            header_2 |= CompactWidth::from_u64(end_time_diff).bitmask(6);
        }

        consumer.consume(header_2).await?;

        if (self.subspaces().start == reference.subspaces().start)
            || (reference.subspaces().end == self.subspaces().start)
        {
            // Don't encode
        } else {
            self.subspaces().start.encode(consumer).await?;
        }

        if self.subspaces().end == RangeEnd::Open
            || (self.subspaces().end == reference.subspaces().start)
            || (self.subspaces().end == reference.subspaces().end)
        {
            // Don't encode end subspace
        } else if let RangeEnd::Closed(end_subspace) = &self.subspaces().end {
            end_subspace.encode(consumer).await?;
        }

        if is_bitflagged(header_1, 4) {
            self.paths()
                .start
                .relative_encode(&reference.paths().start, consumer)
                .await?;
        } else if let RangeEnd::Closed(end_path) = &reference.paths().end {
            self.paths()
                .start
                .relative_encode(end_path, consumer)
                .await?;
        }

        if let RangeEnd::Closed(end_path) = &self.paths().end {
            if is_bitflagged(header_1, 6) {
                end_path
                    .relative_encode(&reference.paths().start, consumer)
                    .await?
            } else if let RangeEnd::Closed(ref_end_path) = &reference.paths().end {
                end_path.relative_encode(ref_end_path, consumer).await?;
            }
        }

        encode_compact_width_be(start_time_diff, consumer).await?;

        if self.times().end != RangeEnd::Open {
            encode_compact_width_be(end_time_diff, consumer).await?;
        }

        Ok(())
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodable<Range3d<MCL, MCC, MPL, S>, DecodingWentWrong> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + Decodable + std::fmt::Debug,
{
    /// Decodes a [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
    ///
    /// Will return an error if the encoding has not been produced by the corresponding encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
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

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodableCanonic<Range3d<MCL, MCC, MPL, S>, DecodingWentWrong, DecodingWentWrong>
    for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
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
            0b1100_0000 => {
                let decoded_subspace = S::decode_canonical(producer).await?;

                if decoded_subspace == reference.subspaces().start
                    || reference.subspaces().end == decoded_subspace
                {
                    return Err(DecodeError::InvalidInput);
                }

                decoded_subspace
            }
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
            _ => {
                let decoded_subspace = RangeEnd::Closed(S::decode_canonical(producer).await?);

                if decoded_subspace == reference.subspaces().start
                    || reference.subspaces().end == decoded_subspace
                {
                    return Err(DecodeError::InvalidInput);
                }

                decoded_subspace
            }
        };

        // Check subspace end...

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

        // Canonicity check for path start
        match &reference.paths().end {
            RangeEnd::Closed(ref_path_end) => {
                let lcp_start_start =
                    path_start.longest_common_prefix(&reference.paths().start);
                let lcp_start_end = path_start.longest_common_prefix(ref_path_end);

                let expected_is_start_rel_to_start =
                    lcp_start_start.component_count() >= lcp_start_end.component_count();

                if expected_is_start_rel_to_start != is_path_start_rel_to_start {
                    return Err(DecodeError::InvalidInput);
                }
            }
            RangeEnd::Open => {
                if !is_path_start_rel_to_start {
                    return Err(DecodeError::InvalidInput);
                }
            }
        }
        // Canonicity check for path start over

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

        // Canonicity check for path end
        match &path_end {
            RangeEnd::Closed(p_end) => match &reference.paths().end {
                RangeEnd::Closed(ref_end) => {
                    let lcp_end_start = p_end.longest_common_prefix(&reference.paths().start);
                    let lcp_end_end = p_end.longest_common_prefix(ref_end);

                    let expected_is_path_end_rel_to_start =
                        lcp_end_start.component_count() >= lcp_end_end.component_count();

                    if expected_is_path_end_rel_to_start != is_path_end_rel_to_start {
                        return Err(DecodeError::InvalidInput);
                    }
                }
                RangeEnd::Open => {}
            },
            RangeEnd::Open => {
                if is_path_end_rel_to_start {
                    return Err(DecodeError::InvalidInput);
                }
            }
        }
        // End canonicity check

        let start_time_diff =
            decode_compact_width_be(start_time_diff_compact_width, producer).await?;

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

        // Canonicity check for start time
        match reference.times().end {
            RangeEnd::Closed(ref_time_end) => {
                let start_to_start = time_start.abs_diff(reference.times().start);
                let start_to_end = time_start.abs_diff(ref_time_end);

                let expected_is_start_rel_to_start = start_to_start <= start_to_end;

                if expected_is_start_rel_to_start != is_time_start_rel_to_start {
                    return Err(DecodeError::InvalidInput);
                }

                let expected_add_or_subtract_start_time_diff = is_time_start_rel_to_start
                    && time_start >= reference.times().start
                    || !expected_is_start_rel_to_start && time_start >= ref_time_end;

                if expected_add_or_subtract_start_time_diff != add_or_subtract_start_time_diff {
                    return Err(DecodeError::InvalidInput);
                }
            }
            RangeEnd::Open => {
                if !is_time_start_rel_to_start {
                    return Err(DecodeError::InvalidInput);
                }

                // I need to very add or subtract time diff here.
                let expected_add_or_subtract_time_diff = time_start >= reference.times().start;

                if expected_add_or_subtract_time_diff != add_or_subtract_start_time_diff {
                    return Err(DecodeError::InvalidInput);
                }
            }
        }
        // End of canonicity check for start time

        let time_end = if is_times_end_open {
            if add_or_subtract_end_time_diff {
                return Err(DecodeError::InvalidInput);
            }

            if is_time_end_rel_to_start {
                return Err(DecodeError::InvalidInput);
            }

            let end_time_diff_compact_width_flags = 0b0000_0011;
            if header_2 & end_time_diff_compact_width_flags != 0b0000_0000 {
                return Err(DecodeError::InvalidInput);
            }

            RangeEnd::Open
        } else {
            let end_time_diff =
                decode_compact_width_be(end_time_diff_compact_width, producer).await?;

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

            let end_to_start = time_end.abs_diff(reference.times().start);
            let end_to_end = match &reference.times().end {
                RangeEnd::Closed(ref_end) => time_end.abs_diff(*ref_end),
                RangeEnd::Open => u64::MAX,
            };

            let expected_is_time_end_rel_to_start = end_to_start <= end_to_end;
            if expected_is_time_end_rel_to_start != is_time_end_rel_to_start {
                return Err(DecodeError::InvalidInput);
            }

            let expected_end_time_diff = core::cmp::min(end_to_start, end_to_end);

            if expected_end_time_diff != end_time_diff {
                return Err(DecodeError::InvalidInput);
            }

            let expected_add_or_subtract_end_time_diff = (is_time_end_rel_to_start
                && time_end >= reference.times().start)
                || (!is_time_end_rel_to_start && time_end >= reference.times().end);

            if expected_add_or_subtract_end_time_diff != add_or_subtract_end_time_diff {
                return Err(DecodeError::InvalidInput);
            }

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

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableKnownSize<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableKnownSize + std::fmt::Debug,
{
    fn relative_len_of_encoding(&self, r: &Range3d<MCL, MCC, MPL, S>) -> usize {
        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableSync<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableSync + std::fmt::Debug,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodableSync<Range3d<MCL, MCC, MPL, S>, DecodingWentWrong>
    for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableSync + std::fmt::Debug,
{
}
