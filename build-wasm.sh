#!/bin/bash

set -e

# Default web features
WEB_FEATURES="js"

usage()
{
cat << EOF
usage: $0 [options]

Build kyaudioreg for WASM.

OPTIONS:
   -h            Show some help
   -c            Run clippy
   -f <features> Specify features (default: $WEB_FEATURES)
EOF
}

ACTION="build"

while getopts "hcf:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        c)
            ACTION="clippy"
            ;;
        f)
            WEB_FEATURES=$OPTARG
            ;;
    esac
done
shift $(($OPTIND - 1))

if [ "x$1" != "x" ]; then
    usage
    exit 1
fi

# Source common build functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/kyaudioreg-common.sh"

# Run action
case "$ACTION" in
    "build")
        kyaudioreg_wasm_do_build "$WEB_FEATURES"
    ;;
    "clippy")
        kyaudioreg_wasm_do_clippy "$WEB_FEATURES"
    ;;
esac
