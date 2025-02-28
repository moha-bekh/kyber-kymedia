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
   -k            Build Kyber dependencies
   -s            Skip native dependencies build
   -c            Run clippy
   -w            Don't spawn cmd.exe when running avserver.exe
   -o <path>     Install the built binaries in the absolute path
EOF
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

BUILD_DEPS=true
BUILD_KYBER_DEPS=false
HIDE_CONSOLE_FEATURE=""
ACTION="build"

while getopts "hksco:w" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        s)
            BUILD_DEPS=false
            ;;
        k)
            BUILD_KYBER_DEPS=true
            ;;
        c)
            ACTION="clippy"
            ;;
        o)
            OUT_DIR=$OPTARG
            ;;
        w)
            HIDE_CONSOLE_FEATURE="--features win32-hide-console"
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

# Build kyber dependencies
if [ "$BUILD_KYBER_DEPS" = true ]; then
    if [[ ! -d libkypc ]]; then
        git_clone "kyber-labs/kyber-streamer/libkypc.git"
    fi

    if [[ ! -d kymux ]]; then
        git_clone "kyber-labs/kyber-streamer/kymux.git"
    fi

    # Cargo config
    echo "Add extra cargo configuration to find libkypc and kymux"

    mkdir -p .cargo
    echo '[patch.crates-io]' > .cargo/config.toml
    echo 'libkypc = { path = "libkypc" }' >> .cargo/config.toml
    echo 'kymux_client = { path = "kymux/kymux_client" }' >> .cargo/config.toml
fi

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
            --examples \
            $HIDE_CONSOLE_FEATURE

        # Install
        mkdir -p $OUT_DIR/bin

        cp \
            target/x86_64-pc-windows-gnu/release/avserver.exe \
            $OUT_DIR/bin

    ;;
    "clippy")
        cargo clippy \
            --target x86_64-pc-windows-gnu \
            -- -D warnings -A clippy::missing_safety_doc
    ;;
esac
