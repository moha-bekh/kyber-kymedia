#!/bin/bash

set -e

BASE_DIR=$(realpath $(dirname $0))

# Workspace directories
WORKSPACE_ROOT=$BASE_DIR/..

# Build configuration
NJOBS=$(grep -c ^processor /proc/cpuinfo)
PLATFORM_NAME="x86_64-w64-mingw32"
CROSS_PREFIX="x86_64-w64-mingw32"

# Intermediate directories
WORK_DIR="$BASE_DIR/work/$PLATFORM_NAME"
BUILD_DIR="$WORK_DIR/build"
SRC_DIR="$WORK_DIR/src"

usage()
{
cat << EOF
usage: $0 -o <path> [options]

Build avserver.

OPTIONS:
   -h            Show some help
   -s            Skip native dependencies build
   -c            Run clippy
   -o <path>     Install the built binaries in the absolute path
EOF
}

BUILD_DEPS=true
ACTION="build"

while getopts "hsco:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        s)
            BUILD_DEPS=false
            ;;
        c)
            ACTION="clippy"
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

# Build dependencies
if [ "$BUILD_DEPS" = true ]; then
    ./contrib/build-win32.sh -o $OUT_DIR
else
    echo "Skip dependencies build"
fi

# Build
export PATH=$OUT_DIR/bin:$PATH
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR=$OUT_DIR/lib/pkgconfig

# Run action
case "$ACTION" in
    "build")
        # Build
        cargo build \
            --target x86_64-pc-windows-gnu \
            --release \
            --bins \
            --examples

        # Install
        mkdir -p $OUT_DIR/bin

        cp \
            target/x86_64-pc-windows-gnu/release/avserver.exe \
            scripts/txproto_stream_audio.lua \
            scripts/txproto_stream_video.lua \
            $OUT_DIR/bin

    ;;
    "clippy")
        cargo clippy \
            --target x86_64-pc-windows-gnu \
            -- -D warnings -A clippy::missing_safety_doc
    ;;
esac
