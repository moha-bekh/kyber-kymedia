# Project Kyber: build-win32.sh
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

#!/bin/bash

set -e

BASE_DIR=$(realpath $(dirname $0))

# Workspace directories
WORKSPACE_ROOT=$BASE_DIR/..
TXPROTO_SRCDIR="$WORKSPACE_ROOT/external/txproto"

# Build configuration
NJOBS=$(grep -c ^processor /proc/cpuinfo)
PLATFORM_NAME="x86_64-w64-mingw32"
CROSS_PREFIX="x86_64-w64-mingw32"

# Intermediate directories
BUILD_DIR="$BASE_DIR/work/$PLATFORM_NAME"
SRC_DIR="$BASE_DIR/work/src"

VULKAN_SDK_VERSION="1.4.313.0"

panic() {
    echo $1
    exit 1
}

test_sha256() {
    local local_path=$1
    local expected_hash=$2

    echo "Expecting hash: $expected_hash"
    echo "Current hash: $(sha256sum $local_path | awk '{print $1}')"
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

    ./configure --prefix=$OUT_DIR \
        --host=$CROSS_PREFIX --cross-prefix=$CROSS_PREFIX- \
        --enable-static \
        --disable-cli

    make -j $NJOBS
    make install

    cd $init_cwd
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

build_opus() {
    local init_cwd=$(pwd)

    local version="1.3.1"
    local uri="https://archive.mozilla.org/pub/opus/opus-$version.tar.gz"
    local archive_root="opus-$version"
    local sha256="65b58e1e25b2a114157014736a3d9dfeaad8d41be1c8179866f144a2fb44ff9d"
    local work_dir="$BUILD_DIR/opus"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get opus sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    patch -p1 < $BASE_DIR/opus/0001-configure-Disable-FORTIFY_SOURCE.patch

    ./configure --host=$CROSS_PREFIX --prefix=$OUT_DIR \
        --disable-extra-programs --disable-shared

    make -j$NJOBS
    make install

    cd $init_cwd
}

build_amf() {
    local init_cwd=$(pwd)

    local version="1.4.33"

    local uri="https://github.com/GPUOpen-LibrariesAndSDKs/AMF/archive/refs/tags/v$version.tar.gz"
    local archive_root="AMF-$version"
    local sha256="86b39d5bd4652338bf7b4e48efec58472bb57a985250bc149e7908534a915c8e"
    local work_dir="$BUILD_DIR/amf"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get amf sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Install
    cd "$work_dir/$archive_root"
    cp -a amf/public/include $OUT_DIR/include/AMF

    cd $init_cwd
}

build_vpl() {
    local init_cwd=$(pwd)

    local version="2.10.1"

    local uri="https://github.com/oneapi-src/oneVPL/archive/refs/tags/v$version.tar.gz"
    local archive_root="libvpl-$version"
    local sha256="524299a7b920ac0de1f6913ca90515858ea3a8ea2daaea60f8e0be62f22c8041"
    local work_dir="$BUILD_DIR/vpl"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get vpl sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    patch -p1 < $WORKSPACE_ROOT/contrib/vpl/0001-Force-static-libstdc-link.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/vpl/0002-cmake-Fix-.pc-generated-file.patch

    mkdir -p build && cd build

    cmake .. \
        -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DCMAKE_BUILD_TYPE=Release \
        -DBUILD_PREVIEW=OFF \
        -DBUILD_TOOLS=OFF \
        -DBUILD_TOOLS_ONEVPL_EXPERIMENTAL=OFF \
        -DINSTALL_EXAMPLE_CODE=OFF \
        -DBUILD_SHARED_LIBS=OFF

    make -j$NJOBS
    make install

    cd $init_cwd
}

build_spirvheaders() {
    local init_cwd=$(pwd)

    local uri="https://github.com/KhronosGroup/SPIRV-Headers/archive/refs/tags/vulkan-sdk-$VULKAN_SDK_VERSION.tar.gz"
    local archive_root="SPIRV-Headers-vulkan-sdk-$VULKAN_SDK_VERSION"
    local sha256="f68be549d74afb61600a1e3a7d1da1e6b7437758c8e77d664909f88f302c5ac1"
    local work_dir="$BUILD_DIR/SPIRV-Headers"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get SPIRV-Headers sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DSPIRV_HEADERS_ENABLE_TESTS=OFF \
        -DCMAKE_BUILD_TYPE=Release

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_spirvtools() {
    local init_cwd=$(pwd)

    local uri="https://github.com/KhronosGroup/SPIRV-Tools/archive/refs/tags/vulkan-sdk-$VULKAN_SDK_VERSION.tar.gz"
    local archive_root="SPIRV-Tools-vulkan-sdk-$VULKAN_SDK_VERSION"
    local sha256="6b60f723345ceed5291cceebbcfacf7fea9361a69332261fa08ae57e2a562005"
    local work_dir="$BUILD_DIR/SPIRV-Tools"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get SPIRV-Tools sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DCMAKE_BUILD_TYPE=Release \
	    -DSPIRV_TOOLS_BUILD_STATIC=OFF \
        -DSPIRV-Headers_SOURCE_DIR="$OUT_DIR" \
	    -DSPIRV_SKIP_EXECUTABLES=ON \
        -DSPIRV_SKIP_TESTS=ON

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_spirvcross() {
    local init_cwd=$(pwd)

    local uri="https://github.com/KhronosGroup/SPIRV-Cross/archive/refs/tags/vulkan-sdk-$VULKAN_SDK_VERSION.tar.gz"
    local archive_root="SPIRV-Cross-vulkan-sdk-$VULKAN_SDK_VERSION"
    local sha256="7d1de24918bea9897753f7561d4d154f68ec89c36bb70c13598222b8039d4212"
    local work_dir="$BUILD_DIR/SPIRV-Cross"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get SPIRV-Cross sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DCMAKE_BUILD_TYPE=Release \
        -DSPIRV_CROSS_SHARED=ON \
        -DSPIRV_CROSS_STATIC=OFF \
        -DSPIRV_CROSS_CLI=OFF \
        -DSPIRV_CROSS_ENABLE_TESTS=OFF \
        -DSPIRV_CROSS_ENABLE_CPP=OFF \
        -DSPIRV_CROSS_ENABLE_REFLECT=OFF

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_glslang() {
    local init_cwd=$(pwd)

    local version="16.0.0"

    local uri="https://github.com/KhronosGroup/glslang/archive/refs/tags/$version.tar.gz"
    local archive_root="glslang-$version"
    local sha256="172385478520335147d3b03a1587424af0935398184095f24beab128a254ecc7"
    local work_dir="$BUILD_DIR/glslang"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get glslang sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"

    patch -p1 < $WORKSPACE_ROOT/contrib/glslang/0001-cmake-Add-pkg-config-support.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/glslang/0002-cmake-Link-to-spirv-tools.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/glslang/0003-pkgconfig-Adjust-linked-libs.patch

    mkdir -p build && cd build
    cmake .. \
        -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake \
        -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DCMAKE_BUILD_TYPE=Release \
        -DGLSLANG_TESTS=OFF \
        -DENABLE_GLSLANG_BINARIES=OFF \

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_libplacebo() {
    local init_cwd=$(pwd)

    PATH_BAK=$PATH
    export PATH=/usr/x86_64-w64-mingw32/bin:$PATH

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

    patch -p1 < $WORKSPACE_ROOT/contrib/libplacebo/0001-glsl-Make-it-build-with-mingw-w64.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/libplacebo/0002-vulkan-Disable-stub.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/libplacebo/0003-utils-libav-initial-attempt-at-d3d11-hwframe-mapping.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/libplacebo/0004-utils-libav-init-proper-d3d11-texture-size-when-mapp.patch

    mkdir -p build && cd build

    export CFLAGS="-I$OUT_DIR/include"
    export WINDRES=x86_64-w64-mingw32-windres

    meson setup \
        --prefix="$OUT_DIR" \
        --cross-file=$WORKSPACE_ROOT/contrib/mingw-w64-meson-toolchain.ini \
        -Dshaderc=disabled \
        -Dd3d11=enabled \
        -Dlcms=disabled \
        -Dvulkan=disabled \
        -Dopengl=disabled \
        -Ddemos=false \
        -Dprefer_static=false \
        ..

    ninja install

    unset WINDRES
    unset CFLAGS

    export PATH=$PATH_BAK

    cd $init_cwd
}

build_ffmpeg() {
    local init_cwd=$(pwd)

    local version="7.1.2"

    local uri="https://ffmpeg.org/releases/ffmpeg-$version.tar.xz"
    local archive_root="ffmpeg-$version"
    local sha256="089bc60fb59d6aecc5d994ff530fd0dcb3ee39aa55867849a2bbc4e555f9c304"
    local work_dir="$BUILD_DIR/ffmpeg"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get ffmpeg sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    patch -p1 < $WORKSPACE_ROOT/contrib/ffmpeg/0001-nvenc-Patch-SPS-when-zerolatency-is-enabled.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/ffmpeg/0002-libavcodec-amfenc-Add-mapping-for-AV_PIX_FMT_BGRA.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/ffmpeg/0001-libavutil-d3d11va-Avoid-to-query-ID3D11VideoDevice-a.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/ffmpeg/0003-libavutil-Implement-monotonic-av_gettime_relative-on.patch

    mkdir -p build
    cd build

    ../configure --prefix=$OUT_DIR \
        --arch=x86_64 --target-os=mingw32 --cross-prefix=$CROSS_PREFIX- --pkg-config=pkg-config \
        --extra-ldflags="-static -static-libgcc" \
        --enable-shared \
        --disable-doc \
        --enable-gpl --enable-libx264 \
        --enable-libopus \
        --disable-vulkan \
        --enable-nonfree --enable-nvenc \
        --enable-amf \
        --enable-libvpl

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_zlib() {
    local init_cwd=$(pwd)

    local version="1.3.1"

    local uri="https://zlib.net/zlib-$version.tar.gz"
    local archive_root="zlib-$version"
    local sha256="9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23"
    local work_dir="$BUILD_DIR/zlib"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get zlib sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    make -j$NJOBS -f win32/Makefile.gcc PREFIX=$CROSS_PREFIX-

    make -j$NJOBS -f win32/Makefile.gcc \
        prefix=$OUT_DIR \
        INCLUDE_PATH=$OUT_DIR/include \
        LIBRARY_PATH=$OUT_DIR/lib \
        BINARY_PATH=$OUT_DIR/bin \
        install

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
        -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake \
        -DCMAKE_INSTALL_PREFIX=$OUT_DIR \
        -DCMAKE_BUILD_TYPE=Release
    make -j$NJOBS
    make install

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
        --cross-file=$WORKSPACE_ROOT/contrib/mingw-w64-meson-toolchain.ini \
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
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR=$OUT_DIR/lib/pkgconfig

# Build dependencies
build_nvcodecheaders
build_x264
build_amf
build_vpl
build_opus
build_spirvheaders
build_spirvtools
build_spirvcross
build_glslang
build_libplacebo
build_zlib # must be before FFmpeg
build_ffmpeg
build_lua
build_txproto
