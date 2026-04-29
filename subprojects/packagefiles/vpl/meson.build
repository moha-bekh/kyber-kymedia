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

project('vpl', 'c', 'cpp', version : '2.10.1')

ext = import('unstable-external_project')

env = environment()
env.set('CROSS_CMAKE_ARGS', '')

if meson.is_cross_build() and host_machine.system() == 'windows'
  cc = meson.get_compiler('c')
  cpp = meson.get_compiler('cpp')
  windres = find_program(host_machine.cpu_family() + '-w64-mingw32-windres')
  env.set('CROSS_CMAKE_ARGS', ' '.join([
    '-DCMAKE_SYSTEM_NAME=Windows',
    '-DCMAKE_SYSTEM_PROCESSOR=' + host_machine.cpu_family(),
    '-DCMAKE_C_COMPILER=' + cc.cmd_array()[0],
    '-DCMAKE_CXX_COMPILER=' + cpp.cmd_array()[0],
    '-DCMAKE_RC_COMPILER=' + windres.full_path(),
    '-DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER',
    '-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY',
    '-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY',
  ]))
endif

p = ext.add_project('configure',
  configure_options: [
    '--prefix=@PREFIX@',
    '--libdir=@PREFIX@/@LIBDIR@',
    '--includedir=@INCLUDEDIR@',
  ],
  env: env,
)

vpl_dep = declare_dependency(
  compile_args: ['-I' + get_option('prefix') / get_option('includedir')],
  link_args: ['-L' + get_option('prefix') / get_option('libdir'), '-lvpl'],
  version: meson.project_version(),
)

meson.override_dependency('vpl', vpl_dep)
