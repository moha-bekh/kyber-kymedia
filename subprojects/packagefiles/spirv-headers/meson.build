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

project('spirv-headers', 'c', 'cpp', version : '1.4.313')

ext = import('unstable-external_project')

env = environment()
env.set('CROSS_CMAKE_ARGS', '')

if meson.is_cross_build()
  cc = meson.get_compiler('c')
  cpp = meson.get_compiler('cpp')
  if host_machine.system() == 'windows'
    env.set('CROSS_CMAKE_ARGS', ' '.join([
      '-DCMAKE_SYSTEM_NAME=Windows',
      '-DCMAKE_SYSTEM_PROCESSOR=' + host_machine.cpu_family(),
      '-DCMAKE_C_COMPILER=' + cc.cmd_array()[0],
      '-DCMAKE_CXX_COMPILER=' + cpp.cmd_array()[0],
      '-DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER',
      '-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY',
      '-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY',
    ]))
  elif host_machine.system() == 'ios'
    # SDKROOT is set in the environment to the iOS SDK, which causes cmake to
    # compile test programs as iOS binaries and then try to run them on macOS —
    # hanging the build.  Passing CMAKE_SYSTEM_NAME=iOS tells cmake it is
    # cross-compiling so it skips all try_run checks.
    sys_root = meson.get_external_property('sys_root', '')
    env.set('CROSS_CMAKE_ARGS', ' '.join([
      '-DCMAKE_SYSTEM_NAME=iOS',
      '-DCMAKE_OSX_ARCHITECTURES=arm64',
      '-DCMAKE_SYSROOT=' + sys_root,
      '-DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY',
      '-DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER',
      '-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY',
      '-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY',
    ]))
  endif
endif

p = ext.add_project('configure',
  configure_options: [
    '--prefix=@PREFIX@',
    '--libdir=@PREFIX@/@LIBDIR@',
    '--includedir=@INCLUDEDIR@',
  ],
  env: env,
)

spirv_headers_dep = p.dependency('SPIRV-Headers').partial_dependency(
  compile_args: true,
  includes: true,
  sources: true,
)

meson.override_dependency('spirv-headers', spirv_headers_dep)
