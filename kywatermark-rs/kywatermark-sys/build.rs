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

fn mingw_include_args(target: &str) -> Vec<String> {
    let triple = target
        .replace("-pc-windows-gnu", "-w64-mingw32")
        .replace("-unknown-windows-gnu", "-w64-mingw32");

    let sysroot_inc = PathBuf::from(format!("/usr/{triple}/include"));
    if sysroot_inc.is_dir() {
        vec![format!("-I{}", sysroot_inc.to_string_lossy())]
    } else {
        Vec::new()
    }
}

fn main() {
    // KYWATERMARK_INCLUDE_DIR / KYWATERMARK_LIB_DIR (rare): point at a custom
    // install prefix when not using pkg-config. Normally pkg-config (driven
    // by PKG_CONFIG_LIBDIR set in build-linux.sh / build-win32.sh) is enough.
    println!("cargo:rerun-if-env-changed=KYWATERMARK_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=KYWATERMARK_LIB_DIR");

    // ---- linking ----
    //
    // pkg-config probe for kywatermark transitively pulls libavutil and
    // libplacebo via its Requires:. Emits -L / -l / -I flags in the right
    // order for left-to-right symbol resolution.
    //
    // FFmpeg dedup (Windows only): FFmpeg is built as static archives, and
    // libtxproto-0.dll re-exports the full FFmpeg (+x264/opus/vpl) API. Emitting
    // both the static libav* archives AND the txproto import library makes ld
    // fail with `multiple definition` and re-bundles ~30 MB of FFmpeg. So on
    // Windows we probe with metadata suppressed, drop every FFmpeg-provided
    // library, and resolve those symbols from -ltxproto (emitted last). The
    // kywatermark archive itself plus libplacebo and its graphics deps (glslang,
    // SPIRV*, spirv-cross, vulkan) are kept as-is — txproto does not re-export
    // those. On Linux FFmpeg is shared (no duplication): keep the direct probe.
    let ffmpeg_from_txproto = [
        "avcodec",
        "avformat",
        "avfilter",
        "avdevice",
        "avutil",
        "swscale",
        "swresample",
        "x264",
        "opus",
        "vpl",
    ];
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let probed = pkg_config::Config::new()
            .cargo_metadata(false)
            .probe("kywatermark")
            .unwrap_or_else(|e| {
                panic!(
                    "pkg-config probe for 'kywatermark' failed: {e}\n\
                 hint: build the meson tree first (./build-win32.sh -k) so \
                 kywatermark.pc lands in $OUT_DIR/lib/pkgconfig, and ensure \
                 PKG_CONFIG_LIBDIR points at it."
                )
            });
        let mut search_paths: Vec<PathBuf> = Vec::new();
        for path in probed.link_paths {
            if !search_paths.contains(&path) {
                search_paths.push(path);
            }
        }
        for path in &search_paths {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
        for l in &probed.libs {
            if !ffmpeg_from_txproto.contains(&l.as_str()) {
                println!("cargo:rustc-link-lib={l}");
            }
        }
        // Every FFmpeg symbol is resolved from the txproto DLL's import library.
        println!("cargo:rustc-link-lib=dylib=txproto");
    } else {
        pkg_config::Config::new()
            .probe("kywatermark")
            .unwrap_or_else(|e| {
                panic!(
                    "pkg-config probe for 'kywatermark' failed: {e}\n\
                 hint: build the meson tree first (./build-linux.sh -k) so \
                 kywatermark.pc lands in $OUT_DIR/lib/pkgconfig, and ensure \
                 PKG_CONFIG_LIBDIR points at it."
                )
            });
    }

    let target = env::var("TARGET").unwrap_or_default();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    } else if target.contains("windows-gnu") {
        println!("cargo:rustc-link-arg=-Wl,-u,__mingwthr_key_dtor,-lmingwthrd");
    }

    // ---- bindgen ----
    //
    // The wrapper.h includes libavutil headers, so we need the same include
    // resolution as libavconv-sys. Re-use AVCONV_INCLUDE_DIR for that, with
    // the same fallback chain.
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest.join("../..");

    let (include_dirs, extra_include): (Vec<PathBuf>, Option<PathBuf>) =
        if let Ok(d) = env::var("KYWATERMARK_INCLUDE_DIR") {
            (vec![PathBuf::from(d)], None)
        } else if let Ok(d) = env::var("AVCONV_INCLUDE_DIR") {
            (vec![PathBuf::from(d)], None)
        } else if let Ok(lib) = env::var("AVCONV_LIB_DIR") {
            (vec![PathBuf::from(&lib).join("../include")], None)
        } else {
            // In-tree default: rootfs/include from a meson install.
            let rootfs_inc = workspace.join("rootfs/include");
            if !rootfs_inc.is_dir() {
                println!(
                    "cargo:warning=kywatermark-sys: include dir {} not found; \
                     set KYWATERMARK_INCLUDE_DIR or AVCONV_INCLUDE_DIR, or run \
                     ./build-linux.sh / build-win32.sh first",
                    rootfs_inc.display()
                );
            }
            (vec![rootfs_inc], None)
        };

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("kywatermark_.*")
        .allowlist_type("kywatermark_.*")
        // libavutil types referenced by the kywatermark API
        .allowlist_type("AVBufferRef")
        .allowlist_type("AVFrame")
        .blocklist_type("__.*")
        .derive_default(true)
        .derive_debug(false);

    for dir in &include_dirs {
        builder = builder.clang_arg(format!("-I{}", dir.to_string_lossy()));
    }
    if let Some(ref gen_dir) = extra_include {
        builder = builder.clang_arg(format!("-I{}", gen_dir.to_string_lossy()));
    }

    let target = env::var("TARGET").unwrap_or_default();
    if !target.is_empty() {
        builder = builder.clang_arg(format!("--target={target}"));
    }

    if target.contains("windows-gnu") {
        for arg in mingw_include_args(&target) {
            builder = builder.clang_arg(arg);
        }
    }

    let bindings = builder.generate().expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
