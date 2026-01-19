#!/bin/bash

set -e

realpath() {
    [[ $1 = /* ]] && echo "$1" || echo "$PWD/${1#./}"
}

BASE_DIR=$(realpath $(dirname $0))
TARGET="ios"

usage()
{
cat << EOF
usage: $0 -o <path> [options] [action]

Build kymedia application for iOS/tvOS/visionOS (kyaudioreg only - avserver not supported).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies
   -o <path>     Install the built binaries in the absolute path (required)
   -t <target>   Use the specified target (default: $TARGET)
                 Supported targets: ios, ios-sim, tvos, tvos-sim, visionos, visionos-sim

ACTIONS:
   build-kyaudioreg     Build kyaudioreg (default)
   clippy-kyaudioreg    Run clippy on kyaudioreg
   test-kyaudioreg      Run tests on kyaudioreg
EOF
}

panic() {
    echo $1
    exit 1
}


git_clone() {
    local REPO=$1
    local BRANCH=$2

    if [[ -z "${CI_JOB_TOKEN}" ]]; then
        local FULL_REPO_URI="git@gitlab.com:${REPO}"
    else
        local FULL_REPO_URI="https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.com/${REPO}"
    fi

    echo "Repo: '${FULL_REPO_URI}'"
    if [[ -z "${BRANCH}" ]]; then
        git clone --recurse-submodules ${FULL_REPO_URI}
    else
        git clone --recurse-submodules ${FULL_REPO_URI} -b $BRANCH
    fi
}

BUILD_KYBER_DEPS=false
ACTION="build-kyaudioreg"

while getopts "hko:t:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        k)
            BUILD_KYBER_DEPS=true
            ;;
        o)
            OUT_DIR=$OPTARG
            ;;
        t)
            TARGET=$OPTARG
        ;;
    esac
done
shift $(($OPTIND - 1))

# Get action from positional argument
if [ "x$1" != "x" ]; then
    ACTION=$1
    shift
fi

# Ensure no extra arguments
if [ "x$1" != "x" ]; then
    echo "Error: Unexpected argument '$1'"
    usage
    exit 1
fi

# Validate action
if [[ "$ACTION" == *"avserver"* ]]; then
    echo "Error: avserver is not supported on iOS/tvOS/visionOS"
    usage
    exit 1
fi

if [ -z ${OUT_DIR} ]; then
    usage
    exit 1
fi

# Create folders
mkdir -p $OUT_DIR

# Build kyber dependencies
if [ "$BUILD_KYBER_DEPS" = true ]; then
    if [[ ! -d libkypc ]]; then
        git_clone kyber.stream/core/kyutil.git
    fi

    if [[ ! -d kymux ]]; then
        git_clone kyber.stream/core/kymux.git
    fi

    # Cargo config
    echo "Add extra cargo configuration to find libkypc and kymux"

    mkdir -p .cargo
    echo '[patch.crates-io]' > .cargo/config.toml
    echo 'kycom = { path = "kymux/kycom" }' >> .cargo/config.toml
    echo 'libkypc = { path = "kyutil/libkypc" }' >> .cargo/config.toml
fi

export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR=$OUT_DIR/lib/pkgconfig

# Source common build functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/kyaudioreg-common.sh"

# Run action
case "$ACTION" in
    "build-kyaudioreg")
        kyaudioreg_ios_do_build "$TARGET" "$OUT_DIR"
        ;;
    "test-kyaudioreg")
        kyaudioreg_ios_do_test "$TARGET"
        ;;
    "clippy-kyaudioreg")
        kyaudioreg_ios_do_clippy "$TARGET"
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
