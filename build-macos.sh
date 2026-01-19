#!/bin/bash

set -e

# Architecture configuration
BUILD_ARCH=`uname -m | cut -d. -f1`
HOST_ARCH="x86_64"

usage()
{
cat << EOF
usage: $0 -o <path> [options]

Build kyaudioreg.

OPTIONS:
   -h            Show some help
   -c            Run clippy
   -t            Run tests
   -o <path>     Install the built binaries in the absolute path
   -a <arch>     Use the specified arch (default: $HOST_ARCH)
EOF
}

panic() {
    echo $1
    exit 1
}

get_cargo_target()
{
    case $1 in
        x86_64)
            echo "x86_64-apple-darwin"
        ;;
        arm64)
            echo "aarch64-apple-darwin"
        ;;
        *)
            echo "Unexpected arch $1"
            exit 1
        ;;
    esac
}

ACTION="build"

while getopts "hcto:a:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
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
        a)
            HOST_ARCH=$OPTARG
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

CARGO_TARGET=`get_cargo_target $HOST_ARCH`

# Source common build functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/kyaudioreg-common.sh"

# Run action
case "$ACTION" in
    "build")
        kyaudioreg_macos_do_build "$CARGO_TARGET" "$OUT_DIR"
    ;;
    "test")
        kyaudioreg_macos_do_test "$CARGO_TARGET"
    ;;
    "clippy")
        kyaudioreg_macos_do_clippy "$CARGO_TARGET"
    ;;
esac
