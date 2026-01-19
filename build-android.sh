#!/bin/bash

set -e

# Architecture configuration
BUILD_ARCH=`uname -m | cut -d. -f1`
HOST_ARCH="arm64"

usage()
{
cat << EOF
usage: $0 -o <path> [options] [action]

Build kymedia application for Android (kyaudioreg only - avserver not supported).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies
   -o <path>     Install the built binaries in the absolute path (required)
   -a <arch>     Use the specified arch (default: $HOST_ARCH)

ACTIONS:
   build-kyaudioreg     Build kyaudioreg (default)
   clippy-kyaudioreg    Run clippy on kyaudioreg
   test-kyaudioreg      Run tests on kyaudioreg
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

BUILD_KYBER_DEPS=false
ACTION="build-kyaudioreg"

while getopts "hko:a:" OPTION
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
        a)
            HOST_ARCH=$OPTARG
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
    echo "Error: avserver is not supported on android"
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

# Source common build functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/kyaudioreg-common.sh"

# Run action
case "$ACTION" in
    "build-kyaudioreg")
        kyaudioreg_android_do_build "$OUT_DIR" "$HOST_ARCH"
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
