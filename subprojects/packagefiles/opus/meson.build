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

project('opus', 'c', version : '1.3.1')

ext = import('unstable-external_project')

configure_opts = [
  '--prefix=@PREFIX@',
  '--libdir=@PREFIX@/@LIBDIR@',
  '--disable-extra-programs',
]

# Static/shared flags depend on target OS, evaluated at configure time
if host_machine.system() in ['windows', 'android']
  configure_opts += ['--enable-static', '--disable-shared']
  if host_machine.system() == 'windows'
    _cross_host = host_machine.cpu_family() + '-w64-mingw32'
  else
    _cross_host = host_machine.cpu_family() + '-linux-android'
  endif
elif host_machine.system() in ['ios', 'tvos', 'visionos']
  # Apple mobile: shared libs are not supported; build static only.
  # Autotools doesn't recognise 'ios'/'tvos'/'visionos', map to 'darwin'.
  configure_opts += ['--enable-static', '--disable-shared']
  _cross_host = host_machine.cpu_family() + '-apple-darwin'
else
  configure_opts += ['--disable-static', '--enable-shared']
  _cross_host = host_machine.cpu_family() + '-' + build_machine.system() + '-' + host_machine.system()
endif

opus_env = environment()
cc = meson.get_compiler('c')
opus_env.set('CC', ' '.join(cc.cmd_array()))

if host_machine.system() == 'android'
  # The NDK uses LLVM tools without the cross-prefix.
  opus_env.set('AR',     find_program('llvm-ar',     required: true).full_path())
  opus_env.set('RANLIB', find_program('llvm-ranlib', required: true).full_path())
  opus_env.set('STRIP',  find_program('llvm-strip',  required: true).full_path())
  opus_env.set('NM',     find_program('llvm-nm',     required: true).full_path())
endif

c_args = get_option('c_args')
if c_args != []
  opus_env.set('CFLAGS', ' '.join(c_args))
endif

c_link_args = get_option('c_link_args')
if c_link_args != []
  opus_env.set('LDFLAGS', ' '.join(c_link_args))
endif

p = ext.add_project('meson-configure',
  configure_options: configure_opts,
  cross_configure_options: ['--host=' + _cross_host],
  env: opus_env,
)

opus_dep = declare_dependency(
  compile_args: ['-I' + get_option('prefix') / get_option('includedir') / 'opus'],
  link_args: ['-L' + get_option('prefix') / get_option('libdir'), '-lopus'],
  version: meson.project_version(),
)

meson.override_dependency('opus', opus_dep)
