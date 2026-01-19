#!/bin/bash

set -e

# Default web features
WEB_FEATURES="js"

usage()
{
cat << EOF
usage: $0 [options] [action]

Build kymedia application for WASM (kyaudioreg only - avserver not supported).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies
   -f <features> Specify features (default: $WEB_FEATURES)

ACTIONS:
   build-kyaudioreg     Build kyaudioreg for WASM (default)
   clippy-kyaudioreg    Run clippy on kyaudioreg for WASM
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

while getopts "hkf:" OPTION
do
    case $OPTION in
        h)
            usage
            exit 1
            ;;
        k)
            BUILD_KYBER_DEPS=true
            ;;
        f)
            WEB_FEATURES=$OPTARG
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
    echo "Error: avserver is not supported for WASM"
    usage
    exit 1
fi

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
        kyaudioreg_wasm_do_build "$WEB_FEATURES"
        ;;
    "clippy-kyaudioreg")
        kyaudioreg_wasm_do_clippy "$WEB_FEATURES"
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
