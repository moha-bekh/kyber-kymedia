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

# From https://github.com/mesonbuild/wrapdb/tree/master

project(
  'lua-5.4',
  'c',
  license: 'MIT',
  meson_version: '>=0.54.0',
  version: '5.4.6',
  default_options: ['c_std=c99', 'warning_level=2'],
)

lua_versions = meson.project_version().split('.')
cc = meson.get_compiler('c')

# Skip bogus warning.
add_project_arguments(
  cc.get_supported_arguments('-Wno-string-plus-int', '-Wno-stringop-overflow'),
  language: 'c',
)

# Platform-specific defines.
is_apple_mobile = host_machine.system() in ['ios', 'tvos', 'visionos']
is_posix = host_machine.system() in [
  'cygwin',
  'darwin',
  'dragonfly',
  'freebsd',
  'gnu',
  'haiku',
  'linux',
  'netbsd',
  'openbsd',
  'sunos',
]
if is_apple_mobile
  # LUA_USE_IOS implies LUA_USE_POSIX + LUA_USE_DLOPEN and stubs out
  # unavailable APIs (system(), popen(), tmpnam()) for iOS/tvOS/visionOS.
  add_project_arguments('-DLUA_USE_IOS', language: 'c')
elif is_posix
  add_project_arguments(
    '-DLUA_USE_POSIX',
    language: 'c',
  )
endif

# Library dependencies.
lua_lib_deps = [
  cc.find_library(
    'm',
    required: false,
  ),
]

if meson.version().version_compare('>= 0.62')
  dl_dep = dependency(
    'dl',
    required: get_option('loadlib'),
  )
else
  dl_dep = cc.find_library(
    'dl',
    required: get_option('loadlib'),
  )
endif

if dl_dep.found()
  lua_lib_deps += dl_dep
  add_project_arguments(
    '-DLUA_USE_DLOPEN',
    language: 'c',
  )
endif

# Interpreter dependencies.
lua_exe_deps = []
lua_exe_args = []

readline_dep = dependency(
  'readline',
  required: get_option('line_editing'),
)
if readline_dep.found()
  lua_exe_deps += readline_dep
  lua_exe_args += '-DLUA_USE_READLINE'
else
  readline_dep = dependency(
    'libedit',
    required: get_option('line_editing'),
  )
endif

lua_sources = [
  'src/lapi.c',
  'src/lauxlib.c',
  'src/lbaselib.c',
  'src/lcode.c',
  'src/lcorolib.c',
  'src/lctype.c',
  'src/ldblib.c',
  'src/ldebug.c',
  'src/ldo.c',
  'src/ldump.c',
  'src/lfunc.c',
  'src/lgc.c',
  'src/linit.c',
  'src/liolib.c',
  'src/llex.c',
  'src/lmathlib.c',
  'src/lmem.c',
  'src/loadlib.c',
  'src/lobject.c',
  'src/lopcodes.c',
  'src/loslib.c',
  'src/lparser.c',
  'src/lstate.c',
  'src/lstring.c',
  'src/lstrlib.c',
  'src/ltable.c',
  'src/ltablib.c',
  'src/ltm.c',
  'src/lundump.c',
  'src/lutf8lib.c',
  'src/lvm.c',
  'src/lzio.c',
]

# Targets.
# iOS/tvOS/visionOS do not support shared libraries; build static.
if is_apple_mobile
  lua_lib = static_library(
    'lua',
    lua_sources,
    dependencies: lua_lib_deps,
    implicit_include_directories: false,
    install: true,
  )
else
  lua_lib = library(
    'lua',
    lua_sources,
    dependencies: lua_lib_deps,
    implicit_include_directories: false,
    install: true,
  )
endif

inc = include_directories('src')
lua_dep = declare_dependency(
  link_with: lua_lib,
  include_directories: inc,
  version: meson.project_version(),
)

meson.override_dependency('lua', lua_dep)
meson.override_dependency('lua-5.4', lua_dep)

pkg = import('pkgconfig')
pkg.generate(
  name: 'lua',
  filebase: 'liblua',
  description: 'Lua programming language',
  version: meson.project_version(),
  libraries: lua_lib,
  install_dir: join_paths(get_option('libdir'), 'pkgconfig'),
)

if get_option('interpreter')
  lua_exe = executable(
    'lua',
    'src/lua.c',
    c_args: lua_exe_args,
    dependencies: [lua_dep, lua_exe_deps],
    export_dynamic: dl_dep.found(),
    implicit_include_directories: false,
  )
endif
