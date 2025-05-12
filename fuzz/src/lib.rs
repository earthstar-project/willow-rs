pub mod path;
pub mod placeholder_params;
pub mod relative_codec_dummy;
pub mod store;
pub mod store_events;

/// Runs a function and crashes the process if the function doesn't terminate within the given time limit.
pub fn limit_time<Fun: FnOnce() -> ()>(millis: u64, fun: Fun) {
    let success = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let success2 = success.clone();

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(millis));
        if !success.load(std::sync::atomic::Ordering::Relaxed) {
            println!("Timeout!");
            std::process::exit(-1);
        }
    });

    fun();

    success2.store(true, std::sync::atomic::Ordering::Relaxed);
}
