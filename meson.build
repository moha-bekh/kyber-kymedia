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

project(
  'kymedia-deps',
  'c',
  version      : '1.0.0',
  meson_version : '>=1.10.0',
  default_options : {
    'c_std': 'c11',
    'cpp_std': 'c++17',
    # Define NDEBUG in release/plain builds so assert()s (and VLC's
    # GL_ASSERT_NOERROR) are compiled out, matching the shipping autotools
    # build. Without this, benign GL errors on the macOS legacy GL context
    # abort the OpenGL vout instead of being ignored.
    'b_ndebug': 'if-release',
  },
)

build_server = get_option('build_server').enable_auto_if(
  host_machine.system() in ['linux', 'windows'],
)

build_client = get_option('build_client').enable_auto_if(true)

_win32_cross = meson.is_cross_build() and host_machine.system() == 'windows'

if build_server.enabled()

  lua_dep = dependency('lua',
    required        : true,
    default_options : {
      'loadlib': false,
      'interpreter': false,
      'line_editing': 'disabled',
    },
  )

  if _win32_cross
    vpl_dep = dependency('vpl',
      required        : true,
      fallback        : ['vpl', 'vpl_dep'],
    )
  else
    vpl_dep = disabler()
  endif

  # These dependencies are declared here to ensure their subprojects are built
  # before libplacebo, which requires them (spirv, vulkan-headers, glslang).
  spirv_headers_dep  = dependency('spirv-headers',
    required : true,
    fallback : ['spirv-headers', 'spirv_headers_dep'],
  )
  spirv_tools_dep    = dependency('SPIRV-Tools',
    required : true,
    fallback : ['spirv-tools', 'spirv_tools_dep'],
  )
  spirv_cross_dep    = dependency('spirv-cross-c-shared',
    required : true,
    fallback : ['spirv-cross', 'spirv_cross_dep'],
  )
  vulkan_headers_dep = dependency('vulkan-headers',
    required : true,
    fallback : ['vulkan-headers', 'vulkan_headers_dep'],
  )
  glslang_dep        = dependency('glslang',
    required : true,
    fallback : ['glslang', 'glslang_dep'],
  )

  _libplacebo_options = {
    'shaderc': 'disabled',
    'lcms': 'disabled',
    'opengl': 'disabled',
    'demos': false,
    'default_library': 'static',
  }
  if _win32_cross
    _libplacebo_options += {'vulkan': 'disabled', 'd3d11': 'enabled'}
  else
    _libplacebo_options += {'vulkan': 'enabled'}
  endif

  libplacebo_dep = dependency('libplacebo',
    required        : true,
    fallback        : ['libplacebo', 'libplacebo_dep'],
    default_options : _libplacebo_options,
  )

endif

# NVIDIA codec headers (NVENC/NVDEC) are only used where NVENC is enabled
# (windows/linux); skip the contrib entirely on macOS/iOS/android.
if host_machine.system() in ['linux', 'windows']
  ffnvcodec_dep = dependency('ffnvcodec', required : true, fallback : ['nv-codec-headers', 'ffnvcodec_dep'])
endif
x264_dep      = dependency('x264',      required : true, fallback : ['x264',             'x264_dep'])
opus_dep      = dependency('opus',      required : true, fallback : ['opus',             'opus_dep'])
zlib_dep      = dependency('zlib',      required : true, fallback : ['zlib',             'zlib_dep'])

if build_client.enabled()
  ogg_dep     = dependency('ogg',       required : true, fallback : ['ogg',              'libogg_dep'])
endif

kyaudioreg_dep = dependency('kyaudioreg', required : false, method : 'pkg-config')
if kyaudioreg_dep.found()
    meson.override_dependency('kyaudioreg', kyaudioreg_dep)
endif
if _win32_cross
  amf_dep     = dependency('AMF',       required : true, fallback : ['amf', 'amf_dep'])
endif

ffmpeg_dep = dependency('ffmpeg', required : true, fallback : ['ffmpeg', 'ffmpeg_dep'])

if build_server.enabled()
  _txproto_options = {'interface': 'disabled'}
  if _win32_cross
    _txproto_options += {
      'xcb': 'disabled',
      'pulse': 'disabled',
      'wayland': 'disabled',
    }
  endif
  txproto = subproject('txproto', default_options : _txproto_options)
endif

vlc_common_options = {
  'avcodec': 'enabled',
  'avformat': 'enabled',
  'swscale': 'enabled',
  'qt': 'disabled',
  'tests': 'disabled',
  # Disabled on all platforms
  'zvbi': 'disabled',
  'vpx': 'disabled',
  'fribidi': 'disabled',
  'live555': 'disabled',
  'projectm': 'disabled',
  'libass': 'disabled',
  'libxml2': 'disabled',
  'goom2': 'disabled',
  'dvdread': 'disabled',
  'dvdnav': 'disabled',
  'bluray': 'disabled',
  'rav1e': 'disabled',
  'mad': 'disabled',
  'png': 'disabled',
  'theoraenc': 'disabled',
  'theoradec': 'disabled',
  'flac': 'disabled',
  'taglib': 'disabled',
  'shout': 'disabled',
  'ncurses': 'disabled',
  'soxr': 'disabled',
  'libchromaprint': 'disabled',
  'medialibrary': 'disabled',
  'lua': 'disabled',
  # 'sparkle': 'disabled',      # --disable-sparkle  (TODO in VLC meson build)
  # 'schroedinger': 'disabled', # --disable-schroedinger  (not in VLC meson build)
  # 'a52': 'disabled',          # --disable-a52  (not in VLC meson build)
  'postproc': 'disabled',
  'daaladec': 'disabled',
  'daalaenc': 'disabled',
  'speexdsp': 'disabled',
  'ebur128': 'disabled',
  'rnnoise': 'disabled',
  # Plugins not needed by the client (built in the WIP but absent from the
  # autotools reference). libogg itself stays enabled for the opus codec.
  'glspectrum': 'disabled',
  'ogg_demux': 'disabled',
  'mux_ogg': 'disabled',
}

vlc_win32_options = {
  'vlc': false,                 # --disable-vlc  (no standalone VLC executable)
  # 'macosx': 'disabled',       # --disable-macosx  (TODO in VLC meson build)
  # 'dca': 'disabled',          # --disable-dca  (not in VLC meson build)
  'faad': 'disabled',
  'skins2': 'disabled',
  'vcd_module': false,
  # 'vlm': false,               # --disable-vlm  (not in VLC meson build)
  'nls': 'disabled',
  # 'notify': 'disabled',       # --disable-notify  (not in VLC meson build)
  'linsys': 'disabled',
  'libva': 'disabled',
  'gme': 'disabled',
  # 'tremor': 'disabled',       # --disable-tremor  (TODO in VLC meson build)
  'vorbis': 'disabled',
  'fluidsynth': 'disabled',
  # 'jack': 'disabled',         # --disable-jack  (TODO in VLC meson build)
  'pulse': 'disabled',
  'mtp': 'disabled',
  'ogg': 'enabled',             # required by the opus codec plugin
  'avformat': 'disabled',       # drop the avformat demux and avio access plugins
  'speex': 'disabled',
  'screen': 'disabled',
  'freetype': 'disabled',
  'dsm': 'disabled',            # --disable-dsm
  'addon_manager': false,
  'chromecast': 'disabled',
  'jpeg': 'disabled',
  'caca': 'disabled',
  'spatialaudio': 'disabled',
  'libsecret': 'disabled',
  'libgcrypt': 'disabled',
  'update-check': 'disabled',
  'videolan_manager': false,
  'stream_outputs': false,
  'dav1d': 'disabled',
  'aom': 'disabled',
  'x264': 'disabled',
  'x265': 'disabled',
  'mpg123': 'disabled',
  'fdk-aac': 'disabled',
  'twolame': 'disabled',
  # 'mfx': 'disabled',          # not in VLC meson_options.txt
  'kate': 'disabled',
  'tiger': 'disabled',
  'samplerate': 'enabled',
  'libmodplug': 'disabled',
  'matroska': 'disabled',
  'libdvbpsi': 'disabled',
  'aribb24': 'disabled',
  'gnutls': 'disabled',
  'srt': 'disabled',
  'upnp': 'disabled',
  'microdns': 'disabled',
  'rsvg': 'disabled',
  'cairo': 'disabled',
  'vulkan': 'disabled',
  'udev': 'disabled',
  # 'libcddb': 'disabled',      # not in VLC meson_options.txt
  # Linux display/audio (disabled for Windows cross-build)
  'alsa': 'disabled',
  'oss': 'disabled',
  'wayland': 'disabled',
  'x11': 'disabled',
  'xcb': 'disabled',
  'dbus': 'disabled',
  'gles2': 'disabled',
  'drm': 'disabled',
}

vlc_linux_options = {
  # 'dca': 'disabled',          # --disable-dca  (not in VLC meson build)
  'faad': 'disabled',
  'qt': 'disabled',
  'skins2': 'disabled',
  'vcd_module': false,
  # 'vlm': false,               # --disable-vlm  (not in VLC meson build)
  'nls': 'disabled',
  # 'notify': 'disabled',       # --disable-notify  (not in VLC meson build)
  'linsys': 'disabled',
  'gme': 'disabled',
  # 'tremor': 'disabled',       # --disable-tremor  (TODO in VLC meson build)
  'vorbis': 'disabled',
  'fluidsynth': 'disabled',     # --disable-fluidsynth
  # 'jack': 'disabled',         # --disable-jack  (TODO in VLC meson build)
  'mtp': 'disabled',
  'speex': 'disabled',
  'screen': 'disabled',
  'freetype': 'disabled',
  'dsm': 'disabled',            # --disable-dsm
  'addon_manager': false,
  'chromecast': 'disabled',
  'jpeg': 'disabled',
  'x264': 'disabled',
  'x265': 'disabled',
  # 'x26410b': 'disabled',      # --disable-x26410b  (TODO in VLC meson build)
  'libplacebo': 'disabled',
  'nfs': 'disabled',
  'gles2': 'enabled',           # GLES2 video output (libgles2 plugin)
  'vdpau': 'enabled',           # VDPAU hardware acceleration stack
  # The PipeWire output does not integrate the kyaudioreg audio regulator,
  # and on PipeWire-backed PulseAudio servers it wins over the pulse output
  # ("refusing to use PipeWire"), bypassing the regulator. Disable it so the
  # pulse output (which instantiates the regulator) is used.
  'pipewire': 'disabled',
}

vlc_apple_options = {
  'vlc': false,                 # --disable-vlc  (no standalone VLC executable)
  # 'macosx': 'disabled',       # --disable-macosx  (TODO in VLC meson build)
  # 'dca': 'disabled',          # --disable-dca  (not in VLC meson build)
  'faad': 'disabled',
  'skins2': 'disabled',
  'vcd_module': false,
  # 'vlm': false,               # --disable-vlm  (not in VLC meson build)
  'nls': 'disabled',
  # 'notify': 'disabled',       # --disable-notify  (not in VLC meson build)
  'linsys': 'disabled',
  'libva': 'disabled',
  'gme': 'disabled',
  # 'tremor': 'disabled',       # --disable-tremor  (TODO in VLC meson build)
  'vorbis': 'disabled',
  'fluidsynth': 'disabled',
  # 'jack': 'disabled',         # --disable-jack  (TODO in VLC meson build)
  'pulse': 'disabled',
  'mtp': 'disabled',
  'speex': 'disabled',
  'screen': 'disabled',
  'freetype': 'disabled',
  'dsm': 'disabled',            # --disable-dsm
  # 'sparkle': 'disabled',      # --disable-sparkle  (TODO in VLC meson build)
  'addon_manager': false,
  'chromecast': 'disabled',
  'jpeg': 'disabled',
  'caca': 'disabled',
  'aom': 'disabled',
  'dav1d': 'disabled',
  'x264': 'disabled',
  'x265': 'disabled',
  'mpg123': 'disabled',
  'twolame': 'disabled',
  'kate': 'disabled',
  'aribb24': 'disabled',
  'aribb25': 'disabled',
  'archive': 'disabled',
  'srt': 'disabled',
  'upnp': 'disabled',
  'libplacebo': 'disabled',
  'nfs': 'disabled',
  'vnc': 'disabled',
  'decklink': 'disabled',
  'samplerate': 'enabled',      # libsamplerate audio resampler (provided via samplerate.wrap)
  'minimal_macosx': 'disabled', # --disable-minimal-macosx (disabled by default in autotools too)
  # 'sqlite': 'disabled',       # --disable-sqlite  (not in VLC meson build)
  'libssh2': 'disabled',
  # 'luac': 'disabled',         # --disable-luac  (not in VLC meson build)
  # 'asdcplib': 'disabled',     # --disable-asdcplib  (not in VLC meson build)
  # 'cddb': 'disabled',         # --disable-cddb  (not in VLC meson build)
  # 'gsm': 'disabled',          # --disable-gsm  (not in VLC meson build)
  'mpc': 'disabled',
  # 'lame': 'disabled',         # --disable-lame  (not in VLC meson build)
  # 'mysofa': 'disabled',       # --disable-mysofa  (not in VLC meson build)
  'sid': 'disabled',
  # 'gpg-error': 'disabled',    # --disable-gpg-error  (not in VLC meson build)
  # 'breakpad': 'disabled',     # --disable-breakpad  (not in VLC meson build)
  # Linux display/audio (disabled for macOS)
  'alsa': 'disabled',
  'oss': 'disabled',
  'wayland': 'disabled',
  'x11': 'disabled',
  'xcb': 'disabled',
  'dbus': 'disabled',
  'gles2': 'disabled',
  'drm': 'disabled',
  'nvdec': 'disabled',
}

vlc_ios_options = vlc_apple_options + {'ogg': 'enabled'}
# ogg enabled: required for the opus codec plugin (opus needs opus_dep AND ogg_dep)
vlc_macos_options = vlc_apple_options + {'ogg': 'enabled'}

vlc_android_options = {
  'vlc': false,
  'faad': 'disabled',
  'qt': 'disabled',
  'skins2': 'disabled',
  'vcd_module': false,
  'nls': 'disabled',
  'linsys': 'disabled',
  'libva': 'disabled',
  'gme': 'disabled',
  'vorbis': 'disabled',
  'fluidsynth': 'disabled',
  'mtp': 'disabled',
  # ogg intentionally NOT disabled: required for the opus codec plugin
  'speex': 'disabled',
  'screen': 'disabled',
  'freetype': 'disabled',
  'dsm': 'disabled',
  'addon_manager': false,
  'chromecast': 'disabled',
  'jpeg': 'disabled',
  'caca': 'disabled',
  'x264': 'disabled',
  'x265': 'disabled',
  'mpg123': 'disabled',
  'twolame': 'disabled',
  'kate': 'disabled',
  'aribb24': 'disabled',
  'srt': 'disabled',
  'upnp': 'disabled',
  'libplacebo': 'disabled',
  'nfs': 'disabled',
  'vnc': 'disabled',
  'decklink': 'disabled',
  'sid': 'disabled',
  'mpc': 'disabled',
  # No NVIDIA/desktop GPU on Android
  'nvdec': 'disabled',
  # No desktop display/audio systems
  'alsa': 'disabled',
  'oss': 'disabled',
  'wayland': 'disabled',
  'x11': 'disabled',
  'xcb': 'disabled',
  'dbus': 'disabled',
  'drm': 'disabled',
  'pulse': 'disabled',
}

vlc_options = vlc_common_options
if host_machine.system() == 'windows'
  vlc_options += vlc_win32_options
elif host_machine.system() == 'linux'
  vlc_options += vlc_linux_options
elif host_machine.system() == 'darwin'
  vlc_options += vlc_macos_options
elif host_machine.system() == 'ios'
  vlc_options += vlc_ios_options
elif host_machine.system() == 'android'
  vlc_options += vlc_android_options
endif



if build_client.enabled()
  vlc_proj = subproject('vlc', required : true, default_options : vlc_options)
endif
