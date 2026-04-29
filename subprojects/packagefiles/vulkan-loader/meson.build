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

project('vulkan-loader', 'c', version : '1.4.313')

ext = import('unstable-external_project')

vulkan_headers_sp = subproject('vulkan-headers', required: true)
vulkan_headers_dist_prefix = vulkan_headers_sp.get_variable('dist_prefix')

env = environment()
env.set('CROSS_CMAKE_ARGS', '')
env.set('MESON_CROSS', '0')
env.set('VULKAN_HEADERS_DIST_PREFIX', vulkan_headers_dist_prefix)
env.set('CFLAGS', '-DWIN32_LEAN_AND_MEAN')

if meson.is_cross_build() and host_machine.system() == 'windows'
  cc = meson.get_compiler('c')
  env.set('MESON_CROSS', '1')
  env.set('CROSS_CMAKE_ARGS', ' '.join([
    '-DCMAKE_SYSTEM_NAME=Windows',
    '-DCMAKE_SYSTEM_PROCESSOR=' + host_machine.cpu_family(),
    '-DCMAKE_C_COMPILER=' + cc.cmd_array()[0],
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

# external_project's p.dependency() builds its link line literally from the
# name we pass (`-l<name>`) it does not read the upstream pc file.  On
# MinGW we need `-lvulkan-1.dll`
_dist_libdir = meson.current_build_dir() / 'dist' + get_option('prefix') / get_option('libdir')

if host_machine.system() == 'windows'
  _vulkan_link_args = ['-L' + _dist_libdir, '-lvulkan-1.dll']
elif not meson.is_cross_build()
  _vulkan_link_args = ['-L' + _dist_libdir, '-lvulkan']
else
  # cross-Linux: libplacebo dlopens vulkan at runtime, no static link needed.
  _vulkan_link_args = []
endif

vulkan_loader_dep = declare_dependency(
  dependencies: [
    vulkan_headers_sp.get_variable('vulkan_headers_dep'),
    # Drag external_project's `make && make install` CustomTarget into the
    # ninja graph for anything that links against us, without pulling in the
    # broken `-lvulkan` p.dependency() would inject.
    p.dependency('vulkan').partial_dependency(sources: true),
  ],
  link_args: _vulkan_link_args,
)

meson.override_dependency('vulkan', vulkan_loader_dep)
meson.override_dependency('vulkan-loader', vulkan_loader_dep)
