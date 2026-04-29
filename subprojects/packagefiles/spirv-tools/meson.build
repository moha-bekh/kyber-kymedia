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

project('spirv-tools', 'c', 'cpp', version : '1.4.313')

ext = import('unstable-external_project')

spirv_headers_sp = subproject('spirv-headers', required: true)

env = environment()
env.set('SPIRV_HEADERS_SRC', get_option('prefix'))
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
    # sys_root is set in the [properties] section of the cross file so that
    # meson.get_external_property() can retrieve it here.
    sys_root = meson.get_external_property('sys_root', '')
    # c_args are set in [built-in options] in the cross file; use get_option()
    # not meson.get_external_property() which only reads [properties].
    # --target= must be a single token ('--target=arm64-apple-ios16.0'), not
    # split into two ('--target' 'arm64-...'), for the extraction below to work.
    c_args = get_option('c_args')
    cc_target = ''
    foreach arg : c_args
      if arg.startswith('--target=')
        cc_target = arg.split('=')[1]
      endif
    endforeach
    cmake_args = [
      '-DCMAKE_SYSTEM_NAME=iOS',
      '-DCMAKE_OSX_ARCHITECTURES=arm64',
      '-DCMAKE_SYSROOT=' + sys_root,
      '-DCMAKE_C_COMPILER=' + cc.cmd_array()[0],
      '-DCMAKE_CXX_COMPILER=' + cpp.cmd_array()[0],
      '-DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY',
      '-DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER',
      '-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY',
      '-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY',
    ]
    if cc_target != ''
      cmake_args += [
        '-DCMAKE_C_COMPILER_TARGET=' + cc_target,
        '-DCMAKE_CXX_COMPILER_TARGET=' + cc_target,
      ]
    endif
    env.set('CROSS_CMAKE_ARGS', ' '.join(cmake_args))
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

spirv_tools_dep = declare_dependency(
  dependencies: [
    p.dependency('SPIRV-Tools'),
    p.dependency('SPIRV-Tools-opt'),
  ],
  version: meson.project_version(),
)

meson.override_dependency('SPIRV-Tools', spirv_tools_dep)
