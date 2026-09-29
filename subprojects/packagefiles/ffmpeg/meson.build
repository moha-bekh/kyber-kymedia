# Project Kyber: meson.build
# Copyright © 2022-2026 Kyber SAS
# SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
#
# This file is both under dual license: AGPLv3 and a Commercial one.
#
# ----
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

project('ffmpeg', 'c', 'cpp', version : '8.1.0')

mod = import('unstable-external_project')

subproject('x264', required: true)
subproject('opus', required: true)
subproject('zlib', required: true)

if host_machine.system() in ['windows', 'linux']
  # NVIDIA codec headers (NVENC/NVDEC) are only used where NVENC is enabled.
  subproject('nv-codec-headers', required: true)
  subproject('vpl', required: true)
  subproject('amf', required: true)
endif

_ext_deps = [
  dependency('x264'),
  dependency('opus'),
  dependency('zlib'),
]
if host_machine.system() in ['windows', 'linux']
  _ext_deps += dependency('ffnvcodec')
endif
if host_machine.system() == 'windows'
  _ext_deps += dependency('vpl')
  # Windows system libraries required by static FFmpeg.
  _cxx = meson.get_compiler('cpp')
  _libstdcxx_a = _cxx.find_library('stdc++', static : true)
  _ext_deps += _libstdcxx_a
  _ext_deps += declare_dependency(link_args : [
    '-lbcrypt', '-lmfuuid', '-lstrmiids',
    '-lsecur32', '-lws2_32',
    '-lpsapi', '-loleaut32', '-lshlwapi', '-lvfw32',
    '-lncrypt', '-lcrypt32',
    '-lm', '-latomic',
  ])
elif host_machine.system() == 'linux'
  _ext_deps += dependency('vpl')
  # Linux system libraries required by static FFmpeg.
  _c = meson.get_compiler('c')
  _linux_optional_libs = [
    'drm', 'va', 'va-drm', 'va-x11', 'vdpau',
    'xcb', 'xcb-shm', 'xcb-shape', 'xcb-xfixes',
    'vulkan', 'X11',
    'asound',
  ]
  foreach _name : _linux_optional_libs
    _dep = _c.find_library(_name, required : false)
    if _dep.found()
      _ext_deps += _dep
    endif
  endforeach
  _ext_deps += dependency('threads')
  _ext_deps += _c.find_library('m', required : false)
  _ext_deps += declare_dependency(link_args : [
    '-ldl', '-latomic',
  ])
elif host_machine.system() in ['darwin', 'ios', 'tvos', 'visionos']
  _c = meson.get_compiler('c')
  _ext_deps += dependency('appleframeworks', modules : [
    'AudioToolbox',
    'CoreAudio',
    'CoreFoundation',
    'CoreMedia',
    'CoreServices',
    'CoreVideo',
    'QuartzCore',
    'Security',
    'VideoToolbox',
  ])
  _bz2_dep = _c.find_library('bz2', required : false)
  if _bz2_dep.found()
    _ext_deps += _bz2_dep
  endif
  _iconv_dep = _c.find_library('iconv', required : false)
  if _iconv_dep.found()
    _ext_deps += _iconv_dep
  endif
  _ext_deps += _c.find_library('m', required : false)
elif host_machine.system() == 'android'
  _ext_deps += declare_dependency(link_args : [
    '-lm', '-lc',
  ])
endif

prefix_opt = get_option('prefix')
libdir_opt = get_option('libdir')

env = environment()
env.prepend('PKG_CONFIG_PATH', prefix_opt / libdir_opt / 'pkgconfig')
if meson.is_cross_build()
  # For cross-builds, also set PKG_CONFIG_LIBDIR so it takes precedence over
  # meson's meson-uninstalled path which meson sets after our env.prepend.
  # PKG_CONFIG_LIBDIR is searched in addition to PKG_CONFIG_PATH.
  env.set('PKG_CONFIG_LIBDIR', prefix_opt / libdir_opt / 'pkgconfig')
  # sys_root in the cross-file causes meson to set PKG_CONFIG_SYSROOT_DIR,
  # which makes pkg-config prepend the sysroot to all -I/-L paths, breaking
  # library detection. Clear it for ffmpeg's configure subprocess.
  env.set('PKG_CONFIG_SYSROOT_DIR', '')
endif

common_conf = [
  # FFmpeg configure ignores the CC env var and requires --cc= explicitly.
  '--cc=' + ' '.join(meson.get_compiler('c').cmd_array()),
  '--host-cc=' + ' '.join(meson.get_compiler('c', native: true).cmd_array()),
  '--prefix=' + prefix_opt,
  '--libdir=' + join_paths(prefix_opt, libdir_opt),
  '--disable-doc',
  '--enable-gpl',
  '--enable-libx264',
  '--enable-libopus',
  '--enable-zlib',
  '--disable-sdl2',
]

# FFmpeg is built as shared libraries on Linux (dynamic .so) and static on
# every other platform (Windows/macOS/iOS/Android ship a single self-contained
# binary that statically bundles FFmpeg).
ffmpeg_shared = host_machine.system() == 'linux'
if ffmpeg_shared
  common_conf += ['--enable-shared', '--disable-static']
else
  common_conf += ['--disable-shared', '--enable-static']
endif

host_cpu = host_machine.cpu_family()

if host_machine.system() == 'android'
  # Android cross-compile via NDK LLVM toolchain; no prefixed binaries exist.
  _ar     = find_program('llvm-ar',     required: true).full_path()
  _ranlib = find_program('llvm-ranlib', required: true).full_path()
  _strip  = find_program('llvm-strip',  required: true).full_path()
  _nm     = find_program('llvm-nm',     required: true).full_path()

  common_conf += [
    '--enable-pic',
    '--extra-cflags=-fPIC',
    '--disable-nvenc',
  ]

  cross_conf = [
    '--enable-cross-compile',
    '--target-os=android',
    '--arch=' + host_cpu,
    '--cross-prefix=',
    '--ar='     + _ar,
    '--ranlib=' + _ranlib,
    '--strip='  + _strip,
    '--nm='     + _nm,
    '--pkg-config=pkg-config',
    '--pkg-config-flags=--static',
    '--extra-libs=-lm',
  ]
elif host_machine.system() == 'windows' and meson.is_cross_build()
  # Windows cross-compile via MinGW.
  common_conf += [
    '--enable-nvenc',
    '--disable-vulkan',
  ]

  cross_conf = [
    '--enable-cross-compile',
    '--target-os=mingw32',
    '--arch=' + host_cpu,
    '--cross-prefix=' + host_cpu + '-w64-mingw32-',
    '--pkg-config=pkg-config',
    '--extra-ldflags=-static-libgcc -L' + join_paths(prefix_opt, libdir_opt),
    '--enable-amf',
    '--enable-libvpl',
    '--enable-libx264',
  ]
elif host_machine.system() == 'linux'
  common_conf += [
    '--enable-pic',
    # No -fvisibility=hidden for the shared build: FFmpeg relies on its
    # generated linker version scripts to export the public av_* API, and
    # hidden visibility would strip those symbols from the .so.
    '--extra-cflags=-fPIC',
    '--enable-nvenc',
    '--enable-libvpl',
    '--enable-libdrm',
    '--enable-vulkan',
    # Disable features autodetected from host -dev packages but unused by VLC;
    # their system libs are not in _ext_deps, so they break the static link.
    '--disable-sndio',
    '--disable-lzma',
    '--disable-bzlib',
    '--disable-xlib',
  ]
  cross_conf = []
elif host_machine.system() in ['ios', 'tvos', 'visionos']
  # Apple mobile cross-compile.
  _c_args = get_option('c_args')
  # Extract --target and --sysroot separately from c_args.
  # Passing --sysroot (GCC-style) in extra-cflags or via ffmpeg's --sysroot=
  # causes ffmpeg to prepend it to all pkg-config -I/-L paths, breaking
  # library detection. Use clang's -isysroot instead: clang does not prepend
  # -isysroot to absolute -I/-L paths from pkg-config.
  _target_flag = ''
  _isysroot_flag = ''
  foreach arg : _c_args
    if arg.startswith('--target=')
      _target_flag = arg
    elif arg.startswith('--sysroot=')
      _isysroot_flag = '-isysroot ' + arg.substring(10)
    endif
  endforeach
  _apple_extra_cflags = []
  _apple_extra_ldflags = []
  if _target_flag != ''
    _apple_extra_cflags += _target_flag
    _apple_extra_ldflags += _target_flag
  endif
  if _isysroot_flag != ''
    _apple_extra_cflags += _isysroot_flag
    _apple_extra_ldflags += _isysroot_flag
  endif
  common_conf += [
    '--disable-nvenc',
    '--disable-vulkan',
    '--enable-videotoolbox',
    '--enable-audiotoolbox',
    '--enable-pic',
    # audiotoolbox indev uses macOS-only CoreAudio HAL APIs not available on iOS
    '--disable-indev=audiotoolbox',
    '--disable-avdevice',
    # bin2c (host tool) is killed when SDKROOT points to the iOS SDK; this
    # Metal-based deinterlace filter is not needed for iOS builds anyway.
    '--disable-filter=yadif_videotoolbox',
  ]
  if _apple_extra_cflags != []
    common_conf += ['--extra-cflags=' + ' '.join(_apple_extra_cflags)]
    common_conf += ['--extra-ldflags=' + ' '.join(_apple_extra_ldflags)]
  endif
  cross_conf = [
    '--enable-cross-compile',
    '--target-os=darwin',
    '--arch=' + host_cpu,
    '--pkg-config=pkg-config',
    '--pkg-config-flags=--static',
  ]
elif host_machine.system() == 'darwin'
  common_conf += [
    '--enable-pic',
    '--extra-cflags=-fPIC -fvisibility=hidden',
    '--disable-nvenc',
    '--enable-videotoolbox',
    '--enable-audiotoolbox',
    # unused, pull unlinked frameworks (AVFoundation/CoreImage/OpenGL/Metal) → break static link
    '--disable-indev=avfoundation',
    '--disable-filter=coreimage',
    '--disable-filter=coreimagesrc',
    '--disable-filter=yadif_videotoolbox',
  ]
  if meson.is_cross_build()
    # macOS cross-arch (e.g. arm64 runner building x86_64): tell FFmpeg configure
    # the real target arch so it doesn't fall back to uname -m (arm64).
    cross_conf = [
      '--enable-cross-compile',
      '--target-os=darwin',
      '--arch=' + host_cpu,
    ]
  else
    cross_conf = []
  endif
else
  cross_conf = []
endif

p = mod.add_project('meson-configure',
  configure_options : common_conf,
  cross_configure_options : cross_conf,
  verbose : true,
  env : env,
)

libs = {
  'libavcodec' : 'avcodec',
  'libavformat' : 'avformat',
  'libavutil' : 'avutil',
  'libswresample' : 'swresample',
  'libswscale' : 'swscale',
  'libavfilter' : 'avfilter',
  'libavdevice' : 'avdevice',
}

_dist_libdir = meson.current_build_dir() / 'dist' + join_paths(prefix_opt, libdir_opt)

wrapped_deps = []

lib_versions = {
  'libavcodec' : '61.19.101',
  'libavformat' : '61.7.100',
  'libavutil' : '59.39.100',
  'libswresample' : '5.3.100',
  'libswscale' : '8.3.100',
  'libavfilter' : '10.4.100',
  'libavdevice' : '61.3.100',
}

# Inter-library dependencies (from FFmpeg's config.mak *_FFLIBS).
# p.dependency() only returns direct Libs: from each .pc file, not the
# transitive deps from Requires:. We must add them explicitly.
_fflibs = {
  'avutil'      : [],
  'swresample'  : ['avutil'],
  'avcodec'     : ['swresample', 'avutil'],
  'swscale'     : ['avutil'],
  'avformat'    : ['avcodec', 'swresample', 'avutil'],
  'avfilter'    : ['swscale', 'avformat', 'avcodec', 'swresample', 'avutil'],
  'avdevice'    : ['avfilter', 'swscale', 'avformat', 'avcodec', 'swresample', 'avutil'],
}

# First pass: collect raw deps (each includes its own system libs from .pc Libs:).
_raw_deps = {}
foreach dep_name, lib_name : libs
  _raw_deps += {lib_name : p.dependency(lib_name)}
endforeach

# Second pass: assemble with inter-lib deps + -L for dist dir, then override.
foreach dep_name, lib_name : libs
  dep_version = lib_versions.get(dep_name, meson.project_version())
  deps = [_raw_deps[lib_name]]
  foreach fflib : _fflibs[lib_name]
    deps += _raw_deps[fflib]
  endforeach
  dep = declare_dependency(
    dependencies : deps + _ext_deps,
    link_args : ['-L' + _dist_libdir],
    version : dep_version,
  )
  wrapped_deps += [dep]
  meson.override_dependency(dep_name, dep)
  if not ffmpeg_shared
    # Static platforms (Windows/macOS/iOS/Android): consumers such as VLC
    # request these with dependency(..., static : true). On Linux FFmpeg is
    # shared and consumers request the default variant, so only the generic
    # override above is registered there.
    meson.override_dependency(dep_name, dep, static : true)
  endif
endforeach

ffmpeg_dep = declare_dependency(dependencies : wrapped_deps)
meson.override_dependency('ffmpeg', ffmpeg_dep)
if not ffmpeg_shared
  meson.override_dependency('ffmpeg', ffmpeg_dep, static : true)
endif
