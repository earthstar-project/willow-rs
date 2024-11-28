use willow_encoding::Encodable;

pub struct SendToChannel<M> {
    channel: u64,
    size: u64,
    message: M,
}

// impl<M: Encodable> Encodable for SendToChannel<M> {
//     fn encode<Consumer>(
//         &self,
//         consumer: &mut Consumer,
//     ) -> impl std::future::Future<Output = Result<(), Consumer::Error>>
//     where
//         Consumer: ufotofu::local_nb::BulkConsumer<Item = u8> {
//         todo!()
//     }
// }