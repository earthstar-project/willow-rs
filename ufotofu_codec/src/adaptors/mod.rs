//! Adaptors which turn consumers of bytes into consumers of encodables, and producers of bytes into producers of decodables.
//!
//! The [`Encoder`] wrapper takes a [`BulkConsumer`](ufotofu::BulkConsumer) of bytes and turns it into a [`BufferedConsumer`](ufotofu::BufferedConsumer) of encodable values. Conversely, the [`Decoder`] wrapper takes a [`BulkProducer`](ufotofu::BulkProducer) of bytes and turns it into a [`BufferedProducer`](ufotofu::BufferedProducer) of decodables; the [`CanonicDecoder`] further enforces canonicity. [`RelativeEncoder`], [`RelativeDecoder`], and [`RelativeCanonicDecoder`] encode and decode respectively relative to some value (which can be freely changed over time).
//!
//! Each of these also has a *shared* counterpart, which operates not on a consumer/producer, but on a reference to a [`Mutex`](wb_async_utils::Mutex). These versions take exclusive access when they start working with an item, to ensure that no other accesses to the underlying consumer/producer interfere with encoding/decoding.

mod encoder;
pub use encoder::*;

mod decoder;
pub use decoder::*;

mod relative_encoder;
pub use relative_encoder::*;

mod relative_decoder;
pub use relative_decoder::*;

mod shared_encoder;
pub use shared_encoder::*;

mod shared_decoder;
pub use shared_decoder::*;

mod shared_relative_encoder;
pub use shared_relative_encoder::*;

mod shared_relative_decoder;
pub use shared_relative_decoder::*;
