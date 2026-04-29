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
#
# Build libsamplerate from its native autotools build system as a static
# library, via meson's external_project module (same pattern as the opus
# contrib). A full native meson port is deferred to later.

project('libsamplerate', 'c', version : '0.2.2')

ext = import('unstable-external_project')

# Always build a static library. Disable the optional sndfile/fftw/alsa
# detection so only the resampler library is built (examples and tests are
# gated on libsndfile in the autotools build and are therefore skipped).
configure_opts = [
  '--prefix=@PREFIX@',
  '--libdir=@PREFIX@/@LIBDIR@',
  '--enable-static',
  '--disable-shared',
  '--disable-sndfile',
  '--disable-fftw',
  '--disable-alsa',
]

# Host triple for cross builds (autotools --host). Mirrors the opus contrib.
if host_machine.system() == 'windows'
  _cross_host = host_machine.cpu_family() + '-w64-mingw32'
elif host_machine.system() == 'android'
  _cross_host = host_machine.cpu_family() + '-linux-android'
elif host_machine.system() in ['ios', 'tvos', 'visionos']
  # Autotools doesn't recognise 'ios'/'tvos'/'visionos', map to 'darwin'.
  _cross_host = host_machine.cpu_family() + '-apple-darwin'
else
  _cross_host = host_machine.cpu_family() + '-' + build_machine.system() + '-' + host_machine.system()
endif

samplerate_env = environment()
cc = meson.get_compiler('c')
samplerate_env.set('CC', ' '.join(cc.cmd_array()))

if host_machine.system() == 'android'
  # The NDK uses LLVM tools without the cross-prefix.
  samplerate_env.set('AR',     find_program('llvm-ar',     required: true).full_path())
  samplerate_env.set('RANLIB', find_program('llvm-ranlib', required: true).full_path())
  samplerate_env.set('STRIP',  find_program('llvm-strip',  required: true).full_path())
  samplerate_env.set('NM',     find_program('llvm-nm',     required: true).full_path())
endif

c_args = get_option('c_args')
if c_args != []
  samplerate_env.set('CFLAGS', ' '.join(c_args))
endif

c_link_args = get_option('c_link_args')
if c_link_args != []
  samplerate_env.set('LDFLAGS', ' '.join(c_link_args))
endif

p = ext.add_project('meson-configure',
  configure_options: configure_opts,
  cross_configure_options: ['--host=' + _cross_host],
  env: samplerate_env,
)

# Header (samplerate.h) installs flat into includedir; static archive is
# libsamplerate.a.
samplerate_dep = declare_dependency(
  compile_args: ['-I' + get_option('prefix') / get_option('includedir')],
  link_args: ['-L' + get_option('prefix') / get_option('libdir'), '-lsamplerate'],
  version: meson.project_version(),
)

meson.override_dependency('samplerate', samplerate_dep)
