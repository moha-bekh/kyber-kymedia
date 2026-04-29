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

project('glslang', 'c', 'cpp', version : '16.0.0')

ext = import('unstable-external_project')

spirv_tools_sp = subproject('spirv-tools', required: true)

env = environment()
env.set('CROSS_CMAKE_ARGS', '')

if meson.is_cross_build() and host_machine.system() == 'windows'
  cc = meson.get_compiler('c')
  cpp = meson.get_compiler('cpp')
  env.set('CROSS_CMAKE_ARGS', ' '.join([
    '-DCMAKE_SYSTEM_NAME=Windows',
    '-DCMAKE_SYSTEM_PROCESSOR=' + host_machine.cpu_family(),
    '-DCMAKE_C_COMPILER=' + cc.cmd_array()[0],
    '-DCMAKE_CXX_COMPILER=' + cpp.cmd_array()[0],
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

glslang_dep = declare_dependency(
  dependencies: [
    p.dependency('glslang'),
    p.dependency('SPIRV'),
    p.dependency('glslang-default-resource-limits'),
  ],
  version: meson.project_version(),
)

spirv_dep = p.dependency('SPIRV')

meson.override_dependency('glslang', glslang_dep)
meson.override_dependency('spirv', spirv_dep)
