use std::env;
use std::sync::Arc;

use log::{warn, LevelFilter};

fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        warn!("Missing script path");
        return;
    }

    let script_name = &args[1];

    let script_args: Option<Vec<_>> = if args.len() > 2 {
        Some(args[2..].iter().map(|i| i.as_str()).collect())
    } else {
        None
    };

    let c = txproto_rs::Context::new(script_name, "main", script_args).unwrap();

    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    c.stop();
}
