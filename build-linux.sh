#!/bin/bash

set -e

BASE_DIR=$(realpath $(dirname $0))

usage()
{
cat << EOF
usage: $0 -o <path> [options] [action]

Build kymedia application (avserver and kyaudioreg).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies (for avserver)
   -s            Skip native dependencies build (for avserver)
   -o <path>     Install the built binaries in the absolute path (required)

ACTIONS:
   build                Build both avserver and kyaudioreg (default)
   build-avserver       Build avserver only
   build-kyaudioreg     Build kyaudioreg only
   clippy               Run clippy on both avserver and kyaudioreg
   clippy-avserver      Run clippy on avserver only
   clippy-kyaudioreg    Run clippy on kyaudioreg only
   test                 Run tests on both avserver and kyaudioreg
   test-avserver        Run tests on avserver only
   test-kyaudioreg      Run tests on kyaudioreg only
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
ACTION="build"

while getopts "hkso:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        k)
            BUILD_KYBER_DEPS=true
            ;;
        s)
            BUILD_DEPS=false
            ;;
        o)
            OUT_DIR=$OPTARG
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

if [ -z ${OUT_DIR} ]; then
    usage
    exit 1
fi

# Create folders
mkdir -p $OUT_DIR

# Build kyber dependencies (only needed for avserver)
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

# Build dependencies (only needed for avserver)
if [[ "$ACTION" == *"avserver"* || "$ACTION" == "build" || "$ACTION" == "clippy" || "$ACTION" == "test" ]] && [ "$BUILD_DEPS" = true ]; then
    ./contrib/build-linux.sh -o $OUT_DIR
else
    echo "Skip dependencies build"
fi

# Source common build functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "$ACTION" == *"avserver"* || "$ACTION" == "build" || "$ACTION" == "clippy" || "$ACTION" == "test" ]]; then
    source "$SCRIPT_DIR/avserver-common.sh"
fi

if [[ "$ACTION" == *"kyaudioreg"* || "$ACTION" == "build" || "$ACTION" == "clippy" || "$ACTION" == "test" ]]; then
    source "$SCRIPT_DIR/kyaudioreg-common.sh"
fi

# Setup environment for avserver
if [[ "$ACTION" == *"avserver"* || "$ACTION" == "build" || "$ACTION" == "clippy" || "$ACTION" == "test" ]]; then
    export PATH=$OUT_DIR/bin:$PATH
    export PKG_CONFIG_PATH=$OUT_DIR/lib/pkgconfig:$OUT_DIR/lib/x86_64-linux-gnu/pkgconfig
    export LD_LIBRARY_PATH=$OUT_DIR/lib:$OUT_DIR/lib/x86_64-linux-gnu
fi

# Run action
case "$ACTION" in
    "build")
        avserver_linux_do_build "$OUT_DIR"
        kyaudioreg_linux_do_build "$OUT_DIR"
        ;;
    "build-avserver")
        avserver_linux_do_build "$OUT_DIR"
        ;;
    "build-kyaudioreg")
        kyaudioreg_linux_do_build "$OUT_DIR"
        ;;
    "clippy")
        avserver_linux_do_clippy
        kyaudioreg_linux_do_clippy
        ;;
    "clippy-avserver")
        avserver_linux_do_clippy
        ;;
    "clippy-kyaudioreg")
        kyaudioreg_linux_do_clippy
        ;;
    "test")
        avserver_linux_do_test "$OUT_DIR"
        kyaudioreg_linux_do_test
        ;;
    "test-avserver")
        avserver_linux_do_test "$OUT_DIR"
        ;;
    "test-kyaudioreg")
        kyaudioreg_linux_do_test
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
