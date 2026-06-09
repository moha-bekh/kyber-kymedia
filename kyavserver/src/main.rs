// Project Kyber: main.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
//
// This file is both under dual license: AGPLv3 and a Commercial one.
//
// ----
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(feature = "win32-hide-console", windows_subsystem = "windows")]

use std::process::ExitCode;

use log::{error, info, LevelFilter};

use kyavservice::Result;

#[cfg(target_os = "windows")]
fn set_process_dpi_aware() {
    use log::{info, warn};
    use windows_sys::Win32::{
        Foundation::{GetLastError, TRUE},
        UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
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
    let pattern_encoder =
        PatternEncoder::new("{d(%Y-%m-%dT%H:%M:%S.%3f)}[{P}] {h({l})} {t} - {m}{n}");

    // Setup Console output
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(pattern_encoder.clone()))
        .build();

    // Compute log file
    let log_folder = "log";
    let log_file = format!("{logfile}.log");

    // KYBER_LOG_DIR overrides the log directory; the file name is unchanged.
    let log_path = if let Some(dir) = std::env::var_os("KYBER_LOG_DIR") {
        Path::new(&dir).join(log_file)
    } else if cfg!(windows) {
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
async fn main() -> ExitCode {
    let (config_table, config_path) = match kyavservice::parse_config() {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("Failed to load configuration: {e}");
            return ExitCode::FAILURE;
        }
    };

    let log_level = if std::env::var("KYBER_VERBOSE").is_ok() {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let _log_handle = setup_logs("kyavserver", log_level);
    match &config_path {
        Some(path) => info!("Using configuration file {path:?}"),
        None => info!("No configuration file found, using defaults"),
    }

    let config = match kyavservice::from_table(&config_table) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to parse configuration: {e}");
            return ExitCode::FAILURE;
        }
    };

    #[cfg(target_os = "windows")]
    set_process_dpi_aware();

    txproto_rs::init_logger();

    match real_main(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("kyavserver exited with error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn real_main(config: kyavservice::Config) -> Result<()> {
    let worker = libkypc::process::connect_to_commander().await?;
    kyavservice::run(worker, config).await
}
