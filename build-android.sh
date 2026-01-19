#!/bin/bash

set -e

usage()
{
cat << EOF
usage: $0 -o <path> -a <arch>

Build kyaudioreg for Android.

OPTIONS:
   -h            Show some help
   -o <path>     Install the built binaries in the absolute path
   -a <arch>     Architecture (arm64, armv7a)
EOF
}

while getopts "ho:a:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
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

if [ -z ${OUT_DIR} ] || [ -z ${HOST_ARCH} ]; then
    usage
    exit 1
fi

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Set KYAUDIOREG_SRCDIR if not set (for workspace builds)
if [ -z ${KYAUDIOREG_SRCDIR} ]; then
    export KYAUDIOREG_SRCDIR="$SCRIPT_DIR"
fi

# If VLC_SRCDIR is set, calculate the VLC contrib path based on architecture
# This is for VLC integration builds
if [ ! -z ${VLC_SRCDIR} ]; then
    case $HOST_ARCH in
        armv7a)
            VLC_CONTRIB="arm-linux-androideabi"
        ;;
        arm64)
            VLC_CONTRIB="aarch64-linux-android"
        ;;
        *)
            echo "Unexpected arch $HOST_ARCH"
            exit 1
        ;;
    esac
    OUT_DIR="$VLC_SRCDIR/contrib/$VLC_CONTRIB"
fi

# Create folders
mkdir -p $OUT_DIR

# Source common build functions
source "$SCRIPT_DIR/kyaudioreg-common.sh"

# Build
kyaudioreg_android_do_build "$OUT_DIR" "$HOST_ARCH"
