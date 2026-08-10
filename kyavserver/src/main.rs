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

use log::{error, info};
use log4ky::notice;

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

#[tokio::main]
async fn main() -> ExitCode {
    let (config_table, config_path) = match kyavservice::parse_config() {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("Failed to load configuration: {e}");
            return ExitCode::FAILURE;
        }
    };

    let log_handle = match log4ky::LoggerBuilder::new("kyavserver")
        .toml(&config_table)
        .base(log4ky::LogConfig {
            include_pid: true,
            ..Default::default()
        })
        .console(!cfg!(feature = "win32-hide-console"))
        .init()
    {
        Ok(h) => h,
        // Config-key warnings already went to stderr and the log file; a
        // hard install failure (no log file exists yet) has not.
        Err(e) => {
            notice!("Failed to initialize logger: {e}");
            return ExitCode::FAILURE;
        }
    };

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

    match real_main(config, log_handle).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("kyavserver exited with error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn real_main(config: kyavservice::Config, log_handle: log4ky::LoggerHandle) -> Result<()> {
    let worker = libkypc::process::connect_to_commander().await?;
    kyavservice::run(worker, config, &log_handle).await
}
