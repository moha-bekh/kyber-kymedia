#!/bin/bash

set -e

BASE_DIR=$(realpath $(dirname $0))

# Workspace directories
WORKSPACE_ROOT=$BASE_DIR/..
TXPROTO_SRCDIR="$WORKSPACE_ROOT/external/txproto"

# Build configuration
NJOBS=$(grep -c ^processor /proc/cpuinfo)
PLATFORM_NAME="x86_64-linux-gnu"
PLATFORM_NAME="x86_64-w64-mingw32"
CROSS_PREFIX="x86_64-w64-mingw32"

# Intermediate directories
BUILD_DIR="$BASE_DIR/work/$PLATFORM_NAME"
SRC_DIR="$BASE_DIR/work/src"

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

build_x264() {
    local init_cwd=$(pwd)

    local uri="https://code.videolan.org/videolan/x264/-/archive/5db6aa6cab1b146e07b60cc1736a01f21da01154/x264-5db6aa6cab1b146e07b60cc1736a01f21da01154.zip"
    local archive_root='x264-5db6aa6cab1b146e07b60cc1736a01f21da01154'
    local sha256="6896825756ebe2c2657580d7636ed5fd6a41fea8e7722ba866fac7be6780d02f"
    local work_dir="$BUILD_DIR/x264"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get x264 sources"
    mkdir -p $work_dir
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
    local sha1="c5e4af74850a616c42d39ed45b9b8568b71bf8bf"
    local work_dir="$BUILD_DIR/nv-codec-headers"

    # Get sources
    mkdir -p $work_dir && cd $work_dir
    git clone $uri
    cd nv-codec-headers
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

    local version="1.4.29"

    local uri="https://github.com/GPUOpen-LibrariesAndSDKs/AMF/archive/refs/tags/v$version.tar.gz"
    local archive_root="AMF-$version"
    local sha256="be42e4acd973fc7a228f087313bee9eaca08df031ec4596f14fb2eabef528628"
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

    local version="2023.1.2"

    local uri="https://github.com/oneapi-src/oneVPL/archive/refs/tags/v$version.tar.gz"
    local archive_root="oneVPL-$version"
    local sha256="b2261cca174ce1815f3d47c647921ab17f68267c69c1e2444ff27aab7d199cbb"
    local work_dir="$BUILD_DIR/vpl"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get vpl sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    patch -p1 < $WORKSPACE_ROOT/contrib/vpl/0001-Force-static-libstdc-link.patch
    patch -p1 < $WORKSPACE_ROOT/contrib/vpl/0002-cmake-Fix-.pc-generated-file.patch

    mkdir build && cd build

    cmake .. \
        -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
        -DBUILD_PREVIEW=OFF -DBUILD_TOOLS=OFF -DBUILD_TOOLS_ONEVPL_EXPERIMENTAL=OFF -DINSTALL_EXAMPLE_CODE=OFF \
        -DBUILD_SHARED_LIBS=OFF

    make -j$NJOBS
    make install

    cd $init_cwd
}

build_ffmpeg() {
    local init_cwd=$(pwd)

    local uri="https://github.com/FFmpeg/FFmpeg.git"
    local sha1="2d202985b79630cd5056c4e32f8f77f22bf1067c"
    local work_dir="$BUILD_DIR/ffmpeg"

    # Get sources
    mkdir -p $work_dir && cd $work_dir
    git clone $uri
    cd FFmpeg
    git checkout $sha1

    # Build
    mkdir build && cd build

    ../configure --prefix=$OUT_DIR \
        --arch=x86_64 --target-os=mingw32 --cross-prefix=$CROSS_PREFIX- --pkg-config=pkg-config \
        --extra-ldflags="-static-libgcc" \
        --enable-shared \
        --enable-gpl --enable-libx264 \
        --enable-libopus \
        --enable-nonfree --enable-nvenc \
        --enable-amf \
        --enable-libvpl

    make -j $NJOBS
    make install

    cd $init_cwd
}

build_zlib() {
    local init_cwd=$(pwd)

    local version="1.2.13"

    local uri="https://zlib.net/zlib-$version.tar.gz"
    local archive_root="zlib-$version"
    local sha256="b3a24de97a8fdbc835b9833169501030b8977031bcb54b3b3ac13740f846ab30"
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

    local version="5.4.4"

    local uri="https://www.lua.org/ftp/lua-$version.tar.gz"
    local archive_root="lua-$version"
    local sha256="164c7849653b80ae67bec4b7473b884bf5cc8d2dca05653475ec2ed27b9ebf61"
    local work_dir="$BUILD_DIR/lua"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get lua sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    patch -p1 < $BASE_DIR/lua/0001-Add-CMakeLists.txt.patch

    mkdir build && cd build
    cmake .. -DCMAKE_TOOLCHAIN_FILE=$WORKSPACE_ROOT/contrib/mingw-w64-toolchain.cmake -DCMAKE_INSTALL_PREFIX=$OUT_DIR
    make -j$NJOBS
    make install

    cd $init_cwd
}

build_txproto() {
    local init_cwd=$(pwd)

    mkdir -p $BUILD_DIR/txproto
    cd $BUILD_DIR/txproto

    meson \
        --buildtype=release \
        --prefix=$OUT_DIR \
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
build_x264
build_nvcodecheaders
build_amf
build_vpl
build_opus
build_ffmpeg
build_lua
build_zlib
build_txproto
