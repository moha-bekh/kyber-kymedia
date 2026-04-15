#!/bin/bash
# Project Kyber: build-linux.sh
# Copyright © 2022-2026 Kyber SAS
# SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0
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

set -e

BASE_DIR=$(realpath $(dirname $0))

# Workspace directories
WORKSPACE_ROOT=$BASE_DIR/..
TXPROTO_SRCDIR="$WORKSPACE_ROOT/external/txproto"
KYAUDIOREG_SRCDIR="$WORKSPACE_ROOT/kyaudioreg"
VLC_SRCDIR="$WORKSPACE_ROOT/external/vlc"

# Build configuration
NJOBS=$(grep -c ^processor /proc/cpuinfo)
PLATFORM_NAME="x86_64-linux-gnu"

# Intermediate directories
BUILD_DIR="$BASE_DIR/work/$PLATFORM_NAME"
SRC_DIR="$BASE_DIR/work/src"

VULKAN_SDK_VERSION="1.4.313"

panic() {
    echo $1
    exit 1
}

test_sha256() {
    local local_path=$1
    local expected_hash=$2

    local real_hash=$(sha256sum $local_path | awk '{print $1}')

    if [[ $real_hash == $expected_hash ]]; then
        return 0
    else
        return 1
    fi
}

download_tarball() {
    local uri=$1
    local tarball_hash=$2

    local filename=$(basename $uri)
    local local_path="$SRC_DIR/$filename"

    echo "Downloading $local_path"

    # Check if file exists and is valid
    if [[ -f $local_path ]]; then
        test_sha256 $local_path $tarball_hash

        if [[ $? -eq 0 ]]; then
            echo "File already exists and is valid"
            return 0
        fi

        echo "File found, but invalid"
        rm $local_path
    fi

    # Get and check file integrity
    wget $uri -O $local_path
    test_sha256 $local_path $tarball_hash
    if [[ $? -eq 0 ]]; then
        return 0
    fi

    return 1
}

build_nvcodecheaders() {
    local init_cwd=$(pwd)

    local uri="https://github.com/FFmpeg/nv-codec-headers.git"
    local sha1="n12.1.14.0"
    local work_dir="$BUILD_DIR/nv-codec-headers"

    # Get sources
    git -C "$work_dir" fetch || git clone "$uri" "$work_dir"
    cd "$work_dir"
    git checkout $sha1

    # Build
    make install PREFIX=$OUT_DIR

    cd $init_cwd
}

build_x264() {
    local init_cwd=$(pwd)

    local uri="https://code.videolan.org/videolan/x264/-/archive/a8b68ebfaa68621b5ac8907610d3335971839d52/x264-a8b68ebfaa68621b5ac8907610d3335971839d52.zip"
    local archive_root='x264-a8b68ebfaa68621b5ac8907610d3335971839d52'
    local sha256="a2146359710182ae252f2125264d564ec82f2b947497b6b03da520bed1e0914b"
    local work_dir="$BUILD_DIR/x264"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get x264 sources"
    rm -rf "$work_dir"
    mkdir "$work_dir"
    unzip "$SRC_DIR/$(basename $uri)" -d $work_dir

    # Build
    cd "$work_dir/$archive_root"

    ./configure --disable-cli --enable-shared --prefix=$OUT_DIR
    make -j $NJOBS
    make install

    cd $init_cwd
}

build_vulkanheaders() {
    local init_cwd=$(pwd)

    local uri="https://github.com/KhronosGroup/Vulkan-Headers/archive/refs/tags/v$VULKAN_SDK_VERSION.tar.gz"
    local archive_root="Vulkan-Headers-$VULKAN_SDK_VERSION"
    local sha256="f3298b8dc620530493296759858a69b622f98ececa0e8c75488ad2000778148f"
    local work_dir="$BUILD_DIR/Vulkan-Headers"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get Vulkan-Headers sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir
    rm "$SRC_DIR/$(basename $uri)"

    # Build
    cd "$work_dir/$archive_root"

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DCMAKE_BUILD_TYPE=Release

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_vulkanloader() {
    local init_cwd=$(pwd)

    local uri="https://github.com/KhronosGroup/Vulkan-Loader/archive/refs/tags/v$VULKAN_SDK_VERSION.tar.gz"
    local archive_root="Vulkan-Loader-$VULKAN_SDK_VERSION"
    local sha256="0c2436993597f5bd0ee420b6b27632758ed3ab439043d251795fd13d4e70a2f3"
    local work_dir="$BUILD_DIR/Vulkan-Loader"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get Vulkan-Loader sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    export CFLAGS=-DWIN32_LEAN_AND_MEAN

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DCMAKE_BUILD_TYPE=Release

    make -j $NJOBS
    make install

    cd $init_cwd
}

# Requires spirv-tools libglade2-dev
build_glslang() {
    local init_cwd=$(pwd)

    local version="15.0.0"

    local uri="https://github.com/KhronosGroup/glslang/archive/refs/tags/$version.tar.gz"
    local archive_root="glslang-$version"
    local sha256="c31c8c2e89af907507c0631273989526ee7d5cdf7df95ececd628fd7b811e064"
    local work_dir="$BUILD_DIR/glslang"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get glslang sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DCMAKE_BUILD_TYPE=Release \
        -DALLOW_EXTERNAL_SPIRV_TOOLS=TRUE \
        -DGLSLANG_TESTS=OFF \
        -DENABLE_GLSLANG_BINARIES=OFF \ 

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_libplacebo() {
    local init_cwd=$(pwd)

    local version="7.351.0"

    local uri="https://github.com/haasn/libplacebo/archive/refs/tags/v$version.tar.gz"
    local archive_root="libplacebo-$version"
    local sha256="716954501d9b76e6906fddda66febc5886493d0673dd265ec1e6e52f4e5cd7c6"
    local work_dir="$BUILD_DIR/libplacebo"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get libplacebo sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    patch -p1 < $WORKSPACE_ROOT/contrib/libplacebo/0005-utils-libav-fix-component-mapping-order-for-BGR-formats.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/libplacebo/0006-vulkan-utils_gen-fix-for-python-3-14.patch

    mkdir -p build && cd build

    export CFLAGS="-I$OUT_DIR/include"

    meson setup \
        --prefix="$OUT_DIR" \
        -Dshaderc=disabled \
        -Dlcms=disabled \
        -Dvulkan=enabled \
        -Dopengl=disabled \
        -Ddemos=false \
        -Dprefer_static=false \
        ..

    ninja install

    unset CFLAGS

    cd $init_cwd
}

build_lua() {
    local init_cwd=$(pwd)

    local version="5.4.6"

    local uri="https://www.lua.org/ftp/lua-$version.tar.gz"
    local archive_root="lua-$version"
    local sha256="7d5ea1b9cb6aa0b59ca3dde1c6adcb57ef83a1ba8e5432c0ecd06bf439b3ad88"
    local work_dir="$BUILD_DIR/lua"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get lua sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    patch -p1 < $BASE_DIR/lua/0001-Add-CMakeLists.txt.patch

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_INSTALL_PREFIX=$OUT_DIR \
        -DCMAKE_BUILD_TYPE=Release
    make -j$NJOBS
    make install

    cd $init_cwd
}

build_ffmpeg() {
    local init_cwd=$(pwd)

    local version="8.1"

    local uri="https://ffmpeg.org/releases/ffmpeg-$version.tar.xz"
    local archive_root="ffmpeg-$version"
    local sha256="b072aed6871998cce9b36e7774033105ca29e33632be5b6347f3206898e0756a"
    local work_dir="$BUILD_DIR/ffmpeg"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get ffmpeg sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    patch -p1 < $BASE_DIR/ffmpeg/0001-nvenc-Patch-SPS-when-zerolatency-is-enabled.patch

    mkdir -p build
    cd build

    ../configure --prefix=$OUT_DIR \
        --enable-shared \
        --disable-doc \
        --enable-libdrm \
        --enable-gpl --enable-libx264 \
        --enable-libopus \
        --enable-vulkan \
        --enable-nvenc

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_kyaudioreg() {
    cd "$KYAUDIOREG_SRCDIR"
    cargo cinstall \
        --release \
        --prefix="$OUT_DIR"
    cd -
}

build_libvlc() {
    local init_cwd=$(pwd)

    # Bootstrap
    cd $VLC_SRCDIR
    ./bootstrap

    # Build
    mkdir -p $BUILD_DIR/vlc
    cd $BUILD_DIR/vlc

    CONFIGFLAGS=$(cat << EOF
--disable-dca --disable-zvbi --disable-vpx --disable-faad --disable-fribidi
--disable-qt --disable-skins2 --disable-vcd --disable-vlm --disable-nls --disable-notify --disable-live555
--enable-swscale --disable-projectm --disable-libass --disable-libxml2 --disable-goom --disable-dvdread
--disable-dvdnav --disable-bluray --disable-linsys --disable-gme --disable-tremor --disable-vorbis
--disable-fluidsynth --disable-jack --disable-mtp --disable-speex --disable-theora
--disable-flac --disable-screen --disable-freetype --disable-taglib --disable-smb2 --disable-sparkle
--disable-addonmanagermodules --disable-mad --disable-chromecast --disable-lua --disable-medialibrary --disable-a52
--disable-rav1e --disable-jpeg --disable-ncurses --disable-chromaprint --disable-shout --disable-schroedinger
--disable-soxr --disable-x264 --disable-x26410b --disable-x265 --disable-libplacebo --disable-nfs
--disable-wayland --disable-vulkan --disable-gst-decode
EOF
)

    $VLC_SRCDIR/configure --prefix=$OUT_DIR $CONFIGFLAGS
    make -j $NJOBS
    make install -j $NJOBS

    cd $init_cwd
}

build_txproto() {
    local init_cwd=$(pwd)

    mkdir -p $BUILD_DIR/txproto
    cd $BUILD_DIR/txproto

    meson setup \
        --buildtype=release \
        --prefix=$OUT_DIR \
        --libdir=$OUT_DIR/lib \
        -Dinterface=disabled \
        $TXPROTO_SRCDIR

    ninja
    ninja install

    cd $init_cwd
}

usage()
{
cat << EOF
usage: $0 -o <path> [options]

Build txproto and its native dependencies.

OPTIONS:
   -h            Show some help
   -o <path>     Install the built binaries in the absolute path
EOF
}

while getopts "ho:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        o)
            OUT_DIR=$OPTARG
            ;;
    esac
done
shift $(($OPTIND - 1))

if [ "x$1" != "x" ]; then
    usage
    exit 1
fi

if [ -z ${OUT_DIR} ]; then
    usage
    exit 1
fi

# Create folders
mkdir -p $SRC_DIR $BUILD_DIR
mkdir -p $OUT_DIR

# Output directory
export PATH=$OUT_DIR/bin:$PATH
export LD_LIBRARY_PATH=$OUT_DIR/lib:$OUT_DIR/lib/x86_64-linux-gnu:$OUT_DIR/lib64
export PKG_CONFIG_PATH=$OUT_DIR/lib/pkgconfig:$OUT_DIR/lib/x86_64-linux-gnu/pkgconfig:$OUT_DIR/lib64/pkgconfig

# Build txproto
build_nvcodecheaders
build_x264
build_vulkanheaders
build_vulkanloader
build_glslang
build_libplacebo
build_ffmpeg
build_lua
build_txproto

# Build libvlc
build_kyaudioreg
build_libvlc
