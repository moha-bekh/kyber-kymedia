#!/bin/bash

set -e

usage()
{
cat << EOF
usage: $0 -o <path> [options]

Build kyaudioreg.

OPTIONS:
   -h            Show some help
   -c            Run clippy
   -o <path>     Install the built binaries in the absolute path
EOF
}

ACTION="build"

while getopts "hco:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
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
mkdir -p $OUT_DIR

# Source common build functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/kyaudioreg-common.sh"

# Run action
case "$ACTION" in
    "build")
        kyaudioreg_win32_do_build "$OUT_DIR"
    ;;
    "clippy")
        kyaudioreg_win32_do_clippy
    ;;
esac
