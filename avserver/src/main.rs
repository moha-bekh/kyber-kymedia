#![cfg_attr(feature = "win32-hide-console", windows_subsystem = "windows")]

use log::LevelFilter;

use avservice::Result;

#[cfg(target_os = "windows")]
fn set_process_dpi_aware() {
    use log::{info, warn};
    use windows_sys::Win32::{
        Foundation::{GetLastError, TRUE},
        UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
    };

    let ret = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if ret == TRUE {
        info!("Process is now DPI aware");
    } else {
        warn!("Failed to make process DPI aware: 0x{:X}", unsafe {
            GetLastError()
        });
    }
}

fn setup_logs(logfile: &str, level: LevelFilter) -> log4rs::Handle {
    use std::path::Path;

    use log4rs::{
        append::{console::ConsoleAppender, file::FileAppender},
        config::{Appender, Config, Root},
        encode::pattern::PatternEncoder,
    };

    // Setup pattern
    let pattern_encoder = PatternEncoder::new("{d(%Y-%m-%dT%H:%M:%S.%3f)} {h({l})} {t} - {m}{n}");

    // Setup Console output
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(pattern_encoder.clone()))
        .build();

    // Compute log file
    let log_folder = "log";
    let log_file = format!("{logfile}.log");

    let log_path = if cfg!(windows) {
        let mut exe_path = std::env::current_exe().unwrap();
        exe_path.pop();

        Path::new(&exe_path).join(log_folder).join(log_file)
    } else {
        Path::new(log_folder).join(log_file)
    };

    // Setup File output
    let file = FileAppender::builder()
        .encoder(Box::new(pattern_encoder))
        .build(log_path)
        .unwrap();

    // Configure logger
    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("file", Box::new(file)))
        .build(
            Root::builder()
                .appender("file")
                .appender("stdout")
                .build(level),
        )
        .unwrap();

    log4rs::init_config(config).unwrap()
}

#[tokio::main]
async fn main() -> Result<()> {
    let _log_handle = setup_logs("avserver", LevelFilter::Info);

    #[cfg(target_os = "windows")]
    set_process_dpi_aware();

    txproto_rs::init_logger();

    let worker = libkypc::process::connect_to_commander().await?;
    avservice::run(worker).await
}
