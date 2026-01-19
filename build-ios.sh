#!/bin/bash

set -e

usage()
{
cat << EOF
usage: $0 -o <path> -t <target> -r <rust_version> [options]

Build kyaudioreg for iOS platforms.

OPTIONS:
   -h                  Show some help
   -o <path>           Install the built binaries in the absolute path
   -t <target>         Target (ios, tvos, visionos)
   -r <rust_version>   Rust version to use
EOF
}

while getopts "ho:t:r:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        o)
            OUT_DIR=$OPTARG
            ;;
        t)
            TARGET=$OPTARG
            ;;
        r)
            RUST_VERSION=$OPTARG
            ;;
    esac
done
shift $(($OPTIND - 1))

if [ "x$1" != "x" ]; then
    usage
    exit 1
fi

if [ -z ${OUT_DIR} ] || [ -z ${TARGET} ] || [ -z ${RUST_VERSION} ]; then
    usage
    exit 1
fi

# Create folders
mkdir -p $OUT_DIR

# Set rust version
rustup override set $RUST_VERSION

# Source common build functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/kyaudioreg-common.sh"

# Build
kyaudioreg_ios_do_build "$TARGET" "$OUT_DIR"
