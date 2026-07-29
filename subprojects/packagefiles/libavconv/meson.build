project('libavconv', 'c', version : '0.1.0', meson_version : '>=1.10.0')

mod = import('unstable-external_project')

# FFmpeg upstream provides libav* — libavconv links against them via pkg-config.
subproject('ffmpeg', required: true)

if host_machine.system() in ['windows', 'linux']
  subproject('vpl', required: true)
  subproject('amf', required: true)
endif

_ffmpeg_deps = [
  dependency('libavcodec'),
  dependency('libavformat'),
  dependency('libavutil'),
  dependency('libswresample'),
  dependency('libswscale'),
  dependency('libavfilter'),
  dependency('libavdevice'),
]

_ext_deps = []

# gstreamer grab provider (optional). configure requires the full set of
# gstreamer-1.0 dev packages (require_pkg_config gstreamer / -app / -allocators
# / -video); enable it only when all are present so a missing GStreamer never
# breaks the build.
_gstreamer_modules = [
  'gstreamer-1.0', 'gstreamer-app-1.0',
  'gstreamer-allocators-1.0', 'gstreamer-video-1.0',
]
_gstreamer_deps = []
_have_gstreamer = false
if host_machine.system() == 'linux'
  _gst_found = []
  _gst_all = true
  foreach _m : _gstreamer_modules
    _d = dependency(_m, required : false)
    _gst_found += _d
    _gst_all = _gst_all and _d.found()
  endforeach
  if _gst_all
    _gstreamer_deps = _gst_found
    _have_gstreamer = true
  endif
endif

if host_machine.system() == 'windows'
  _ext_deps += dependency('vpl')
  _cxx = meson.get_compiler('cpp')
  _libstdcxx_a = _cxx.find_library('stdc++', static : true)
  _ext_deps += _libstdcxx_a
  _ext_deps += declare_dependency(link_args : [
    '-lbcrypt', '-lmfuuid', '-lstrmiids',
    '-lsecur32', '-lws2_32',
    '-lpsapi', '-loleaut32', '-lshlwapi', '-lvfw32',
    '-lm', '-latomic',
  ])
elif host_machine.system() == 'linux'
  _ext_deps += dependency('vpl')
  _c = meson.get_compiler('c')
  _linux_optional_libs = [
    'drm', 'va', 'va-drm', 'va-x11', 'vdpau',
    'xcb', 'xcb-shm', 'xcb-randr', 'xcb-shape', 'xcb-xfixes',
    'vulkan', 'X11',
    'wayland-client',
    'asound', 'pulse',
  ]
  foreach _name : _linux_optional_libs
    _dep = _c.find_library(_name, required : false)
    if _dep.found()
      _ext_deps += _dep
    endif
  endforeach
  _ext_deps += _gstreamer_deps
  _ext_deps += declare_dependency(link_args : [
    '-lpthread', '-ldl',
    '-lm', '-latomic',
  ])
elif host_machine.system() in ['darwin', 'ios', 'tvos', 'visionos']
  _c = meson.get_compiler('c')
  _ext_deps += dependency('appleframeworks', modules : [
    'AudioToolbox',
    'CoreAudio',
    'CoreFoundation',
    'CoreMedia',
    'CoreServices',
    'CoreVideo',
    'QuartzCore',
    'Security',
    'VideoToolbox',
  ])
  _ext_deps += declare_dependency(link_args : [
    '-lpthread',
    '-lm',
  ])
elif host_machine.system() == 'android'
  _ext_deps += declare_dependency(link_args : [
    '-lm', '-lc',
  ])
endif

prefix_opt = get_option('prefix')
libdir_opt = get_option('libdir')

env = environment()
env.prepend('PKG_CONFIG_PATH', prefix_opt / libdir_opt / 'pkgconfig')
if meson.is_cross_build()
  # Prepend (not set) so the local dist pkgconfig takes priority WITHOUT wiping a
  # sysroot PKG_CONFIG_LIBDIR provided by the cross environment — otherwise the
  # autodetected target libs (vaapi, libdrm, libxcb, vulkan, ...) become invisible
  # to pkg-config and their providers are silently dropped from the build.
  env.prepend('PKG_CONFIG_LIBDIR', prefix_opt / libdir_opt / 'pkgconfig')
  env.set('PKG_CONFIG_SYSROOT_DIR', '')
endif

common_conf = [
  '--prefix=' + prefix_opt,
  '--libdir=' + join_paths(prefix_opt, libdir_opt),
  '--disable-shared',
  '--enable-static',
  '--disable-doc',
  '--disable-programs',
  '--enable-gpl',
  '--enable-nonfree',
  '--pkg-config=pkg-config',
]

host_cpu = host_machine.cpu_family()

if host_machine.system() == 'android'
  _cc = meson.get_compiler('c')
  _cc_path = _cc.cmd_array()[0]
  _ar     = find_program('llvm-ar',     required: true).full_path()
  _ranlib = find_program('llvm-ranlib', required: true).full_path()
  _strip  = find_program('llvm-strip',  required: true).full_path()
  _nm     = find_program('llvm-nm',     required: true).full_path()

  common_conf += [
    '--enable-pic',
    '--extra-cflags=-fPIC',
    '--disable-ffnvcodec',
    '--disable-vulkan',
  ]

  cross_conf = [
    '--enable-cross-compile',
    '--target-os=android',
    '--arch=' + host_cpu,
    '--cross-prefix=',
    '--cc='     + _cc_path,
    '--ar='     + _ar,
    '--ranlib=' + _ranlib,
    '--strip='  + _strip,
    '--nm='     + _nm,
    '--pkg-config-flags=--static',
    '--extra-libs=-lm',
  ]
elif host_machine.system() == 'windows' and meson.is_cross_build()
  common_conf += [
    '--disable-vulkan',
    '--enable-libvpl',
  ]

  cross_conf = [
    '--enable-cross-compile',
    '--target-os=mingw32',
    '--arch=' + host_cpu,
    '--cross-prefix=' + host_cpu + '-w64-mingw32-',
    '--extra-ldflags=-static-libgcc',
    '--pkg-config-flags=--define-prefix',
  ]
elif host_machine.system() == 'linux'
  common_conf += [
    '--enable-pic',
    '--extra-cflags=-fPIC -fvisibility=hidden',
    '--extra-ldflags=-Wl,-rpath-link,' + join_paths(prefix_opt, libdir_opt),
    '--enable-libvpl',
    '--enable-libpulse',
    '--enable-libxcb',
    '--enable-libv4l2',
  ]
  # Wayland grab provider: libwayland_client is autodetected; the protocol code is
  # generated by configure itself (via wayland-scanner) from a FLAT dir of XMLs it
  # reads as <dir>/<proto>.xml. wlr-screencopy is vendored (shipped by no
  # wayland-protocols release) and on its own already satisfies the provider. The
  # ext-image-copy-capture set + linux-dmabuf are located in the system package via
  # the wayland module, but only when it is recent enough (>= 1.38, where
  # ext-image-copy-capture landed) so older systems degrade to the wlr path instead
  # of failing the build (find_protocol is fatal on a missing protocol).
  _wl_scanner = find_program('wayland-scanner', native : true, required : false)
  if _wl_scanner.found()
    wl = import('wayland')
    _wl_proto_files = [files('wayland-protocols/wlr-screencopy-unstable-v1.xml')]
    _wl_protos = dependency('wayland-protocols', version : '>=1.38', required : false)
    if _wl_protos.found()
      _wl_proto_files += [
        wl.find_protocol('ext-foreign-toplevel-list', state : 'staging', version : 1),
        wl.find_protocol('ext-image-capture-source',  state : 'staging', version : 1),
        wl.find_protocol('ext-image-copy-capture',    state : 'staging', version : 1),
        wl.find_protocol('linux-dmabuf',              state : 'stable',  version : 1),
      ]
    endif
    # Symlink the resolved XMLs into one flat dir (paths passed as argv, never
    # interpolated) before the external project's configure runs.
    _wl_flat_dir = meson.current_build_dir() / 'wayland-protocols'
    run_command(
      find_program('sh'), '-c',
      'set -e; dst="$1"; shift; mkdir -p "$dst"; for x in "$@"; do ln -sf "$x" "$dst/"; done',
      'flatten-wayland-protocols', _wl_flat_dir, _wl_proto_files,
      check : true,
    )
    common_conf += [
      '--wayland-scanner=' + _wl_scanner.full_path(),
      '--wayland-protocols=' + _wl_flat_dir,
    ]
  endif
  if _have_gstreamer
    common_conf += '--enable-gstreamer'
  endif
  cross_conf = []
elif host_machine.system() in ['ios', 'tvos', 'visionos']
  _cc = meson.get_compiler('c')
  _c_args = get_option('c_args')
  _target_flag = ''
  _isysroot_flag = ''
  foreach arg : _c_args
    if arg.startswith('--target=')
      _target_flag = arg
    elif arg.startswith('--sysroot=')
      _isysroot_flag = '-isysroot ' + arg.substring(10)
    endif
  endforeach
  _apple_extra_cflags = []
  _apple_extra_ldflags = []
  if _target_flag != ''
    _apple_extra_cflags += _target_flag
    _apple_extra_ldflags += _target_flag
  endif
  if _isysroot_flag != ''
    _apple_extra_cflags += _isysroot_flag
    _apple_extra_ldflags += _isysroot_flag
  endif
  common_conf += [
    '--disable-ffnvcodec',
    '--disable-vulkan',
    '--enable-pic',
  ]
  if _apple_extra_cflags != []
    common_conf += ['--extra-cflags=' + ' '.join(_apple_extra_cflags)]
    common_conf += ['--extra-ldflags=' + ' '.join(_apple_extra_ldflags)]
  endif
  cross_conf = [
    '--enable-cross-compile',
    '--target-os=darwin',
    '--arch=' + host_cpu,
    '--cc=' + ' '.join(_cc.cmd_array()),
    '--pkg-config-flags=--static',
  ]
elif host_machine.system() == 'darwin'
  common_conf += [
    '--enable-pic',
    '--extra-cflags=-fPIC -fvisibility=hidden',
    '--disable-ffnvcodec',
  ]
  cross_conf = []
else
  cross_conf = []
endif

p = mod.add_project('meson-configure',
  configure_options : common_conf,
  cross_configure_options : common_conf + cross_conf,
  verbose : true,
  env : env,
)

_dist_libdir = meson.current_build_dir() / 'dist' + join_paths(prefix_opt, libdir_opt)

libs = {
  'libavconv' : 'avconv',
  'libavgrab' : 'avgrab',
}

foreach dep_name, lib_name : libs
  deps = [p.dependency(lib_name)] + _ffmpeg_deps + _ext_deps
  if lib_name == 'avgrab'
    deps += [p.dependency('avconv')]
  endif
  dep = declare_dependency(
    dependencies : deps,
    link_args : ['-L' + _dist_libdir],
    version : meson.project_version(),
  )
  meson.override_dependency(dep_name, dep)
  meson.override_dependency(dep_name, dep, static : true)
endforeach
