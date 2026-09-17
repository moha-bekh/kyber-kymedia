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

/// Clang `-I` flags for the MinGW cross sysroot headers (`<windows.h>`,
/// `<d3d11.h>`, ...).
///
/// clang's `--target=...-windows-gnu` already auto-detects the mingw sysroot,
/// so this is just a safety net for setups where it doesn't. We deliberately
/// do NOT add gcc's fixed-headers dir (`/usr/lib/gcc/<triple>/<ver>/include`):
/// its `stddef.h` / `stdarg.h` collide with clang's own builtin versions
/// (`max_align_t` redefinition, `va_*` macro redefinitions) and break bindgen.
/// That dir was previously hardcoded to gcc 12 and only ever "worked" because
/// the path didn't exist on gcc 13+ toolchains, so it was silently skipped.
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
    // AVCONV_INCLUDE_DIR: path to the installed include prefix
    //   e.g. $OUT_DIR/include set by build-win32.sh.
    //   This directory must contain libavconv/, libavgrab/, all libav*
    //   headers, and the generated avconfig.h.
    //   Fallback: $AVCONV_LIB_DIR/../include (derived automatically).
    //   Default (no env vars): the include dirs pkg-config reports for
    //   libavgrab / libavcodec (same PKG_CONFIG_LIBDIR used for linking
    //   below).
    //
    // AVCONV_LIB_DIR: path to the directory containing the compiled
    //   .so / .a / .lib files (e.g. $OUT_DIR/lib set by build-win32.sh).
    //   When absent no extra library search path is added.

    println!("cargo:rerun-if-env-changed=AVCONV_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=AVCONV_LIB_DIR");

    // ---- linking ----
    //
    // Resolve the full link chain (av* libs + third-party deps like opus, x264,
    // zlib, vpl, and Windows system libs) from the Meson-generated .pc files.
    //
    // Probing libavgrab transitively pulls every av* library via Requires:, and
    // libavcodec.pc's Libs: carries all third-party / system libraries.
    // pkg-config emits flags in dependency order, which satisfies GNU ld
    // left-to-right symbol resolution (libavgrab before libavconv, etc.).
    //
    // Requires PKG_CONFIG_LIBDIR (or PKG_CONFIG_PATH) to point at the rootfs's
    // lib/pkgconfig, and PKG_CONFIG_ALLOW_CROSS=1 when cross-compiling.
    // build-win32.sh and build-linux.sh already set both.
    //
    // FFmpeg dedup (Windows only): FFmpeg is built as static archives, and
    // libtxproto-0.dll re-exports the full FFmpeg (+x264/opus/vpl) API. Emitting
    // both the static libav* archives AND the txproto import library makes ld
    // fail with `multiple definition of av_guess_format` (both define the
    // symbol), and re-bundles ~30 MB of FFmpeg. So on Windows we probe with
    // metadata suppressed, drop every FFmpeg-provided library, and resolve those
    // symbols from -ltxproto instead (emitted last, after the libavconv/libavgrab
    // archives that reference them). libavconv/libavgrab's own archives and the
    // genuine system libraries are kept, in pkg-config order.
    //
    // On Linux FFmpeg is a set of shared objects (no static duplication) and
    // libtxproto is not a single fat DLL, so keep the original direct probe.
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
    let dedup_ffmpeg_via_txproto = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");

    let mut pkg_config_include_paths: Vec<PathBuf> = Vec::new();

    if dedup_ffmpeg_via_txproto {
        let mut search_paths: Vec<PathBuf> = Vec::new();
        let mut keep_libs: Vec<String> = Vec::new();
        for lib in &["libavgrab", "libavcodec"] {
            let probed = pkg_config::Config::new()
                .cargo_metadata(false)
                .probe(lib)
                .unwrap_or_else(|e| {
                    panic!(
                        "pkg-config probe for '{lib}' failed: {e}\n\
                         hint: set PKG_CONFIG_LIBDIR to the rootfs's lib/pkgconfig, \
                         and PKG_CONFIG_ALLOW_CROSS=1 when cross-compiling"
                    )
                });
            for path in probed.link_paths {
                if !search_paths.contains(&path) {
                    search_paths.push(path);
                }
            }
            for path in probed.include_paths {
                if !pkg_config_include_paths.contains(&path) {
                    pkg_config_include_paths.push(path);
                }
            }
            // Preserve order and repeats of the kept libraries (they matter for
            // GNU ld's circular resolution); only the FFmpeg archives are dropped.
            for l in probed.libs {
                if !ffmpeg_from_txproto.contains(&l.as_str()) {
                    keep_libs.push(l);
                }
            }
        }
        for path in &search_paths {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
        for l in &keep_libs {
            println!("cargo:rustc-link-lib={l}");
        }
        // Every FFmpeg symbol is resolved from the txproto DLL's import library.
        println!("cargo:rustc-link-lib=dylib=txproto");
    } else {
        // Probe every library whose symbols libavconv-rs references directly:
        // libavgrab (avgrab_*), libavconv (avconv_*), libavformat (av_guess_format,
        // avio_*) and libavcodec. A dynamic probe only exposes each .pc's public
        // Libs (-lavgrab / -lavconv / ...), never the siblings carried in
        // Requires.private, so each must be probed to reach the link line.
        for lib in &["libavgrab", "libavconv", "libavcodec", "libavformat"] {
            let probed = pkg_config::Config::new().probe(lib).unwrap_or_else(|e| {
                panic!(
                    "pkg-config probe for '{lib}' failed: {e}\n\
                     hint: set PKG_CONFIG_LIBDIR to the rootfs's lib/pkgconfig, \
                     and PKG_CONFIG_ALLOW_CROSS=1 when cross-compiling"
                )
            });
            for path in probed.include_paths {
                if !pkg_config_include_paths.contains(&path) {
                    pkg_config_include_paths.push(path);
                }
            }
        }
    }

    // libvpl.a (statically linked via libavcodec) calls Windows Registry APIs
    // (RegOpenKeyExW, RegCloseKey, ...) that live in advapi32. Neither vpl.pc
    // nor the ffmpeg-generated .pc files advertise advapi32, and pkg-config's
    // own output reorders any late-added -l flag ahead of -lvpl. Emit it here
    // last so it sits after -lvpl on the link line and GNU ld can resolve.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }

    let include_dirs: Vec<PathBuf> = if let Ok(d) = env::var("AVCONV_INCLUDE_DIR") {
        vec![PathBuf::from(d)]
    } else if let Ok(lib) = env::var("AVCONV_LIB_DIR") {
        // Derive include from lib: $AVCONV_LIB_DIR/../include
        vec![PathBuf::from(&lib).join("../include")]
    } else {
        if pkg_config_include_paths.is_empty() {
            println!(
                "cargo:warning=libavconv-sys: no include dirs reported by \
                 pkg-config for 'libavgrab' / 'libavcodec'; set AVCONV_INCLUDE_DIR \
                 (or AVCONV_LIB_DIR), or run ./build-linux.sh / build-win32.sh first"
            );
        }
        pkg_config_include_paths
    };

    // ---- C log shim ----
    //
    // Tiny C bridge so FFmpeg's va_list-based log callback never crosses the
    // FFI boundary: av_log_format_line2() formats into a buffer C-side and we
    // hand Rust only (level, *const c_char). Sidesteps both the unstable Rust
    // va_list ABI and the `__.*` type blocklist below.
    println!("cargo:rerun-if-changed=log_shim.c");
    let mut shim = cc::Build::new();
    shim.file("log_shim.c");
    shim.std("c11"); // log_shim.c uses <stdatomic.h> for the sink pointer
    for dir in &include_dirs {
        shim.include(dir);
    }
    shim.compile("kyav_log_shim");

    // ---- bindgen ----

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // allowlist: libavconv
        .allowlist_function("avconv_.*")
        .allowlist_type("AVConv.*")
        .allowlist_type("AVCONV_.*")
        .allowlist_var("AVCONV_.*")
        // allowlist: libavgrab
        .allowlist_function("avgrab_.*")
        .allowlist_type("AVGrab.*")
        .allowlist_type("AVGRAB_.*")
        .allowlist_var("AVGRAB_.*")
        // libavutil / libavcodec items needed by the wrapper
        .allowlist_function("av_log_set_level")
        .allowlist_var("AV_LOG_.*")
        .allowlist_function("av_hwdevice_ctx_create")
        .allowlist_function("av_buffer_ref")
        .allowlist_function("av_buffer_unref")
        .allowlist_function("av_refstruct_unref")
        .allowlist_function("av_refstruct_ref")
        .allowlist_function("av_refstruct_ref_c")
        .allowlist_function("av_strerror")
        .allowlist_function("av_opt_serialize")
        .allowlist_function("av_opt_set.*")
        .allowlist_function("av_set_options_string")
        .allowlist_function("av_guess_format")
        .allowlist_function("avio_open")
        .allowlist_function("avio_closep")
        .allowlist_var("AVIO_FLAG_.*")
        .allowlist_function("avcodec_find_encoder.*")
        .allowlist_type("AVBufferRef")
        .allowlist_type("AVCodec")
        .allowlist_type("AVCodecContext")
        .allowlist_type("AVOutputFormat")
        .allowlist_type("AVMediaType")
        .allowlist_type("AVHWDeviceType")
        .allowlist_type("AVRational")
        .allowlist_type("AVChannelLayout")
        .allowlist_type("AVColorSpace")
        .allowlist_type("AVColorRange")
        .allowlist_type("AVPixelFormat")
        .allowlist_type("AVCodecParameters")
        .allowlist_type("AVPacket")
        .allowlist_type("AVConvCodecParams")
        .allowlist_type("AVConvMuxerHooks")
        .allowlist_type("AVConvMuxHookFlagsIn")
        .allowlist_type("AVConvMuxHookFlagsOut")
        .allowlist_type("AVConvFGHooks")
        .allowlist_type("AVConvFGHookFlagsIn")
        .allowlist_type("AVConvFGHookFlagsOut")
        .allowlist_var("AVCONV_FG_HOOK_FLAG_.*")
        .allowlist_var("AV_CODEC_FLAG_.*")
        .allowlist_var("AV_HWDEVICE_TYPE_.*")
        .allowlist_var("AV_PIX_FMT_.*")
        .allowlist_type("AVCodecID")
        .allowlist_var("AV_PKT_FLAG_.*")
        .allowlist_var("AVERROR.*")
        .allowlist_var("AV_TIME_BASE.*")
        .allowlist_function("av_gettime")
        .allowlist_function("av_gettime_relative")
        .allowlist_var("AV_NOPTS_VALUE")
        .allowlist_var("AV_OPT_SEARCH_.*")
        .allowlist_var("AVMEDIA_TYPE_.*")
        .allowlist_var("AVCONV_HANDLE_PAYLOAD_.*")
        .allowlist_var("AVCONV_MUX_HOOK_FLAG_.*")
        .allowlist_var("AVGRAB_SOURCE_TYPE_.*")
        .allowlist_var("AVGRAB_SOURCE_FLAG_.*")
        // suppress warnings for types we don't need
        .blocklist_type("__.*")
        .derive_default(true)
        .derive_debug(false);

    // Add include path(s) so clang can find libavconv/, libavgrab/,
    // all libav* headers, and generated files like avconfig.h.
    for dir in &include_dirs {
        builder = builder.clang_arg(format!("-I{}", dir.to_string_lossy()));
    }

    // For cross-compilation: tell clang the target triple so it picks the
    // right ABI and system headers.
    let target = env::var("TARGET").unwrap_or_default();
    if !target.is_empty() {
        builder = builder.clang_arg(format!("--target={target}"));
    }

    // For mingw cross-compilation on Linux: add MinGW system includes so
    // clang can find <windows.h> and friends pulled in by the D3D11 headers.
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
