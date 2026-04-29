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

project('x264', 'c', version : '0.164.3107')

ext = import('unstable-external_project')

common_conf = [
  '--prefix=@PREFIX@',
  '--libdir=@PREFIX@/@LIBDIR@',
  '--includedir=@PREFIX@/@INCLUDEDIR@',
  '--enable-static',
  '--enable-pic',
  '--disable-cli',
]

if host_machine.system() == 'windows'
  _cross_host = host_machine.cpu_family() + '-w64-mingw32'
  _cross_prefix = _cross_host + '-'
elif host_machine.system() in ['ios', 'macos', 'darwin']
  # Apple toolchains don't use cross-prefix, tools are configured in the Meson toolchain
  # x264's configure only recognizes 'darwin', not 'ios' or 'macos'
  _apple_sys = host_machine.system()
  if _apple_sys == 'ios' or _apple_sys == 'macos'
    _apple_sys = 'darwin'
  endif
  _cross_host = host_machine.cpu_family() + '-apple-' + _apple_sys
  _cross_prefix = ''
else
  _cross_host = host_machine.cpu_family() + '-' + build_machine.system() + '-' + host_machine.system()
  _cross_prefix = _cross_host + '-'
endif

x264_env = environment()
cc = meson.get_compiler('c')

# x264's configure script splits CC/CFLAGS/LDFLAGS on whitespace, so any element
# containing a space (e.g. an SDK path) must be shell-quoted before joining.
_cc_parts = []
foreach _part : cc.cmd_array()
  _cc_parts += _part.contains(' ') ? '\'' + _part + '\'' : _part
endforeach
x264_env.set('CC', ' '.join(_cc_parts))

# For Android NDK, use LLVM tools without cross-prefix
if host_machine.system() == 'android'
  x264_env.set('AR',     find_program('llvm-ar',      required: true).full_path())
  x264_env.set('RANLIB', find_program('llvm-ranlib',  required: true).full_path())
  x264_env.set('STRIP',  find_program('llvm-strip',   required: true).full_path())
  x264_env.set('STRINGS',find_program('llvm-strings', required: true).full_path())
endif

c_args = get_option('c_args')
if c_args != []
  _c_args_parts = []
  foreach _part : c_args
    _c_args_parts += _part.contains(' ') ? '\'' + _part + '\'' : _part
  endforeach
  _c_args_str = ' '.join(_c_args_parts)
  x264_env.set('CFLAGS', _c_args_str)
  # x264 uses clang as its assembler on Apple ARM targets (iOS, tvOS, etc) and
  # needs the same -arch / -target / sysroot flags as CC so that assembly
  # objects carry the right platform tag.  On x86_64 darwin x264 picks up NASM
  # instead; NASM does not understand clang flags like -arch, so we must not
  # forward c_args to ASFLAGS in that case.
  if host_machine.cpu_family() != 'x86_64'
    x264_env.set('ASFLAGS', _c_args_str)
  endif
endif

c_link_args = get_option('c_link_args')
if c_link_args != []
  _c_link_parts = []
  foreach _part : c_link_args
    _c_link_parts += _part.contains(' ') ? '\'' + _part + '\'' : _part
  endforeach
  x264_env.set('LDFLAGS', ' '.join(_c_link_parts))
endif

p = ext.add_project('meson-configure',
  configure_options: common_conf,
  cross_configure_options: ['--host=' + _cross_host, '--cross-prefix=' + _cross_prefix],
  env: x264_env,
)

x264_dep = declare_dependency(
  compile_args: ['-I' + get_option('prefix') / get_option('includedir')],
  link_args: ['-L' + get_option('prefix') / get_option('libdir'), '-lx264'],
  version: meson.project_version(),
)

meson.override_dependency('x264', x264_dep)
