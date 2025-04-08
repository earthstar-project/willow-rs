#![no_main]

use std::sync::Arc;
use std::thread;

use futures::join;
use libfuzzer_sys::fuzz_target;

use either::Either::*;

use ufotofu::consumer::{self, BulkConsumerOperation};
use ufotofu::producer::{self, BulkProducerOperation};
use ufotofu::{BufferedConsumer, Consumer, Producer};
use wb_async_utils::spsc::*;

fuzz_target!(|data: (
    Box<[u8]>,
    Result<i16, i16>,
    usize,
    Vec<BulkConsumerOperation>,
    Vec<BulkProducerOperation>
)| {
    let (input, last, queue_capacity, consume_ops, produce_ops) = data;

    let queue_capacity = std::cmp::min(2048, std::cmp::max(1, queue_capacity));

    pollster::block_on(async {
        let state: State<ufotofu_queues::Fixed<u8>, i16, i16> =
            State::new(ufotofu_queues::Fixed::new(queue_capacity));
        let (sender, receiver) = new_spsc(&state);
        let mut sender = consumer::BulkScrambler::new(sender, consume_ops);
        let mut receiver = producer::BulkScrambler::new(receiver, produce_ops);

        let send = async {
            for datum in input.iter() {
                assert_eq!(Ok(()), sender.consume(*datum).await);
            }
            assert_eq!(Ok(()), sender.flush().await);

            match last {
                Ok(fin) => {
                    assert_eq!(Ok(()), sender.close(fin).await);
                }
                Err(err) => {
                    sender.as_mut().cause_error(err);
                }
            }
        };

        let receive = async {
            for datum in input.iter() {
                assert_eq!(Ok(Left(*datum)), receiver.produce().await);
            }

            match last {
                Ok(fin) => {
                    assert_eq!(Ok(Right(fin)), receiver.produce().await);
                }
                Err(err) => {
                    assert_eq!(Err(err), receiver.produce().await);
                }
            }
        };

        // let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        // let done_as_well = done.clone();

        // thread::spawn(move || {
        //     thread::sleep(std::time::Duration::from_millis(10000));

        //     if !done_as_well.load(std::sync::atomic::Ordering::Relaxed) {
        //         std::process::exit(-123);
        //     }
        // });

        join!(send, receive);

        // done.store(true, std::sync::atomic::Ordering::Relaxed);
    });
});
