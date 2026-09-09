// Project Kyber: build.rs
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

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");

    let mut bindings = bindgen::Builder::default()
        .header("wrapper.h")
        // For no_std
        .use_core()
        // Use libc
        .ctypes_prefix("libc")
        .allowlist_function("av_.*")
        .allowlist_function("tx_.*")
        .allowlist_function("sp_.*")
        .allowlist_type("AV_.*")
        .allowlist_type("SPLogLevel.*")
        .allowlist_type("IOType.*")
        .allowlist_type("IOSysEntry.*")
        .allowlist_type("TxWatermarkPosition")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mingw_prefix =
            env::var("MINGW_PREFIX").unwrap_or_else(|_| "/usr/x86_64-w64-mingw32".to_string());
        let include_dir = PathBuf::from(mingw_prefix).join("include");
        if include_dir.is_dir() {
            bindings = bindings.clang_arg(format!("-I{}", include_dir.display()));
        } else if let Ok(output) = std::process::Command::new("brew")
            .args(["--prefix", "mingw-w64"])
            .output()
            && output.status.success()
        {
            let prefix = String::from_utf8(output.stdout).expect("brew output not utf8");
            bindings = bindings.clang_arg(format!(
                "-I{}/toolchain-x86_64/mingw/include",
                prefix.trim()
            ));
        }
    }

    // Link against the txproto shared library only. Its import library resolves
    // the tx_*/sp_* API together with the handful of libavutil functions we call
    // (av_dict_*, av_buffer_*, av_strerror), which libtxproto-0.dll re-exports.
    //
    // `.statik(false)` is essential: txproto.pc lists the full static FFmpeg
    // chain (libavcodec, libavformat, libavfilter, ...) under `Libs.private`.
    // A static probe expands those and bundles a second, full copy of FFmpeg
    // (~30 MB) into kyavserver.exe on top of the one already inside
    // libtxproto-0.dll. Forcing a dynamic probe uses only the public
    // `Libs: -ltxproto`; the DLL is self-contained, so the executable needs
    // nothing beyond its import library.
    let txproto = pkg_config::Config::new()
        .statik(false)
        .probe("txproto")
        .unwrap();
    for include_path in &txproto.include_paths {
        bindings = bindings.clang_arg(format!("-I{}", include_path.display()));
    }

    // libavutil is required for its headers (bindgen sees av_* / AV_*
    // declarations through the txproto wrapper).
    //
    // Cargo link metadata is suppressed on Windows, where libtxproto-0.dll
    // re-exports the libavutil functions we call and this probe would otherwise
    // emit `-lavutil` against the static archive. On Linux libtxproto.so
    // re-exports nothing, so our own av_dict_* / av_buffer_* / av_strerror calls
    // need `-lavutil` on the link line: reaching it only through
    // libtxproto.so's DT_NEEDED makes ld fail with "DSO missing from command
    // line".
    let libavutil = pkg_config::Config::new()
        .cargo_metadata(env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows"))
        .probe("libavutil")
        .unwrap();
    for include_path in &libavutil.include_paths {
        bindings = bindings.clang_arg(format!("-I{}", include_path.display()));
    }

    let bindings = bindings.generate().expect("Unable to generate bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
