#!/bin/bash

set -e

BASE_DIR=$(realpath $(dirname $0))

usage()
{
cat << EOF
usage: $0 -o <path> [options]

Build avserver.

OPTIONS:
   -h            Show some help
   -s            Skip native dependencies build
   -c            Run clippy
   -t            Run tests
   -o <path>     Install the built binaries in the absolute path
EOF
}

panic() {
    echo $1
    exit 1
}

BUILD_DEPS=true
ACTION="build"

while getopts "hscto:" OPTION
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
        t)
            ACTION="test"
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
mkdir -p $OUT_DIR

# Build dependencies
if [ "$BUILD_DEPS" = true ]; then
    ./contrib/build-linux.sh -o $OUT_DIR
else
    echo "Skip dependencies build"
fi

# Output directory
export PATH=$OUT_DIR/bin:$PATH
export PKG_CONFIG_PATH=$OUT_DIR/lib/pkgconfig:$OUT_DIR/lib/x86_64-linux-gnu/pkgconfig
export LD_LIBRARY_PATH=$OUT_DIR/lib:$OUT_DIR/lib/x86_64-linux-gnu

# Run action
case "$ACTION" in
    "build")
        # Build
        cargo build --release --bins --examples

        # Install
        mkdir -p $OUT_DIR/bin

        cp \
            target/release/avserver \
            scripts/avserver_rtp_stream.lua \
            $OUT_DIR/bin
    ;;
    "clippy")
        cargo clippy -- -D warnings -A clippy::missing_safety_doc
    ;;
    "test")
        export LD_LIBRARY_PATH=$OUT_DIR/lib:$OUT_DIR/lib/x86_64-linux-gnu
        if [ -z $KYBER_TEST_NO_DEFAULT_FEATURES ]; then
            cargo test
        else
            cargo test --no-default-features
        fi
    ;;
esac
