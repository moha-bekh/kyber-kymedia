# Project Kyber: meson.build
# Copyright © 2022-2026 Kyber SAS
# SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
#
# kywatermark - generic GPU watermark compositor on top of libplacebo.
#
# This is in-tree Kyber code, included via subdir() from the top-level
# meson.build. Not a meson subproject - those are reserved for external
# dependencies under subprojects/.

cc = meson.get_compiler('c')

deps = [
  dependency('libavutil',  fallback : ['ffmpeg',     'libavutil_dep']),
  dependency('libavcodec', fallback : ['ffmpeg',     'libavcodec_dep']),
  dependency('libswscale', fallback : ['ffmpeg',     'libswscale_dep']),
  dependency('libplacebo', fallback : ['libplacebo', 'libplacebo_dep']),
  cc.find_library('m', required : false),
]

sources = [
  'src/watermark.c',
  # PR2 will add: 'src/watermark_vulkan.c'
]

incdirs = include_directories('.')

# Static-only on purpose. On Windows mingw, building a DLL would auto-export
# every libavutil symbol kywatermark statically links (av_log_set_level,
# av_buffer_ref, av_gettime...), and the resulting libkywatermark.dll.a would
# collide at final link time with the same symbols pulled in from
# libavutil.a by other crates (kyavserver). Static avoids the re-export.
kywatermark_lib = static_library('kywatermark',
  sources,
  include_directories : incdirs,
  dependencies        : deps,
  install             : true,
)

install_headers('kywatermark/watermark.h', subdir : 'kywatermark')

pkg = import('pkgconfig')
pkg.generate(kywatermark_lib,
  name        : 'kywatermark',
  description : 'Generic GPU watermark compositor on top of libplacebo',
  requires    : ['libavutil', 'libplacebo'],
)

kywatermark_dep = declare_dependency(
  link_with           : kywatermark_lib,
  include_directories : incdirs,
  dependencies        : deps,
)
meson.override_dependency('kywatermark', kywatermark_dep)
