#!/bin/bash

set -e

BASE_DIR=$(realpath $(dirname $0))

# Workspace directories
WORKSPACE_ROOT=$BASE_DIR/..
TXPROTO_SRCDIR="$WORKSPACE_ROOT/external/txproto"

# Build configuration
NJOBS=$(grep -c ^processor /proc/cpuinfo)
PLATFORM_NAME="x86_64-linux-gnu"

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

build_nvcodecheaders() {
    local init_cwd=$(pwd)

    local uri="https://github.com/FFmpeg/nv-codec-headers.git"
    local sha1="n12.1.14.0"
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

build_x264() {
    local init_cwd=$(pwd)

    local uri="https://code.videolan.org/videolan/x264/-/archive/a8b68ebfaa68621b5ac8907610d3335971839d52/x264-a8b68ebfaa68621b5ac8907610d3335971839d52.zip"
    local archive_root='x264-a8b68ebfaa68621b5ac8907610d3335971839d52'
    local sha256="113c3ca73da52be3f3dc9ad78a3af945ee7a1059ee1f9ee34caa2e18504eb245"
    local work_dir="$BUILD_DIR/x264"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get x264 sources"
    mkdir -p $work_dir
    unzip "$SRC_DIR/$(basename $uri)" -d $work_dir

    # Build
    cd "$work_dir/$archive_root"

    ./configure --disable-cli --enable-shared --prefix=$OUT_DIR
    make -j $NJOBS
    make install

    cd $init_cwd
}

build_ffmpeg() {
    local init_cwd=$(pwd)

    local version="6.0"

    local uri="https://ffmpeg.org/releases/ffmpeg-$version.tar.xz"
    local archive_root="ffmpeg-$version"
    local sha256="57be87c22d9b49c112b6d24bc67d42508660e6b718b3db89c44e47e289137082"
    local work_dir="$BUILD_DIR/ffmpeg"

    # Get sources
    download_tarball $uri $sha256 || panic "Fail to get ffmpeg sources"
    mkdir -p $work_dir
    tar xf "$SRC_DIR/$(basename $uri)" -C $work_dir

    # Build
    cd "$work_dir/$archive_root"
    patch -p1 < $BASE_DIR/ffmpeg/0001-rtpenc-Log-AVPacket-pts-RTP-timestamp-mapping-at-sen.patch
    patch -p1 < $BASE_DIR/ffmpeg/0001-avcodec-x86-mathops-clip-constants-used-with-shift-i.patch
    patch -p1 < $BASE_DIR/ffmpeg/0001-nvenc-Patch-SPS-when-zerolatency-is-enabled.patch
    patch -p1 < $BASE_DIR/ffmpeg/0004-nvenc-Add-an-yuv444p-for-hevc-and-av1-to-force-outpu.patch

    mkdir build && cd build

    ../configure --prefix=$OUT_DIR \
        --enable-shared \
        --disable-doc \
        --enable-libdrm \
        --enable-gpl --enable-libx264 \
        --enable-libopus \
        --enable-nonfree --enable-nvenc

    make -j $NJOBS
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
export LD_LIBRARY_PATH=$OUT_DIR/lib:$OUT_DIR/lib/x86_64-linux-gnu
export PKG_CONFIG_PATH=$OUT_DIR/lib/pkgconfig:$OUT_DIR/lib/x86_64-linux-gnu/pkgconfig

# Build dependencies
build_nvcodecheaders
build_x264
build_ffmpeg
build_txproto
