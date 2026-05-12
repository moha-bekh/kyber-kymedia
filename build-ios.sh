#!/bin/bash
# Project Kyber: build-ios.sh
# Copyright © 2022-2026 Kyber SAS
# SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
#
# This file is both under dual license: AGPLv3 and a Commercial one.
#
# ----
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

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

Build kymedia application for iOS/tvOS/visionOS (kyaudioreg only - kyavserver not supported).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies
   -o <path>     Install the built binaries in the absolute path (required)
   -t <target>   Use the specified target (default: $TARGET)
                 Supported targets: ios, ios-sim, tvos, tvos-sim, visionos, visionos-sim

ACTIONS:
   build                Build kyaudioreg (default)
   test                 Run tests on kyaudioreg
   clippy               Run clippy on kyaudioreg
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
        local FULL_REPO_URI="git@gitlab.com:${ROOT_NAMESPACE}/${REPO}"
    else
        local FULL_REPO_URI="https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.com/${ROOT_NAMESPACE}/${REPO}"
    fi

    echo "Repo: '${FULL_REPO_URI}'"
    if [[ -z "${BRANCH}" ]]; then
        git clone --recurse-submodules ${FULL_REPO_URI}
    else
        git clone --recurse-submodules ${FULL_REPO_URI} -b $BRANCH
    fi
}

BUILD_KYBER_DEPS=false
ACTION="build"

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
if [[ "$ACTION" == *"kyavserver"* ]]; then
    echo "Error: kyavserver is not supported on iOS/tvOS/visionOS"
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
    if [[ ! -d kyutil ]]; then
        git_clone core/kyutil.git
    fi

    if [[ ! -d kymux ]]; then
        git_clone core/kymux.git
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

# iOS build functions
get_cargo_target() {
    case $1 in
        ios) echo "aarch64-apple-ios" ;;
        ios-sim) echo "aarch64-apple-ios-sim" ;;
        tvos) echo "aarch64-apple-tvos" ;;
        tvos-sim) echo "aarch64-apple-tvos-sim" ;;
        visionos) echo "aarch64-apple-visionos" ;;
        visionos-sim) echo "aarch64-apple-visionos-sim" ;;
        *)
            echo "Unexpected target $1"
            exit 1
            ;;
    esac
}

get_cargoc_flags() {
    case $1 in
        ios|ios-sim) echo "" ;;
        tvos|tvos-sim|visionos|visionos-sim)
            echo "-Zbuild-std=std,panic_abort"
            ;;
        *)
            echo "Unexpected target $1"
            exit 1
            ;;
    esac
}

do_build() {
    local target=$1
    local out_dir=$2

    local cargo_target=$(get_cargo_target "$target")
    local cargoc_flags=$(get_cargoc_flags "$target")

    cd kyaudioreg
    cargo cinstall \
        --target=$cargo_target \
        $cargoc_flags \
        --release \
        --library-type=staticlib \
        --prefix="$out_dir"
    cd -
}

do_test() {
    local target=$1

    local cargo_target=$(get_cargo_target "$target")
    local cargoc_flags=$(get_cargoc_flags "$target")

    cd kyaudioreg
    cargo test \
        --target=$cargo_target \
        $cargoc_flags \
        --release \
        --features capi
    cd -
}

do_clippy() {
    local target=$1

    local cargo_target=$(get_cargo_target "$target")
    local cargoc_flags=$(get_cargoc_flags "$target")

    cd kyaudioreg
    cargo clippy \
        --target=$cargo_target \
        $cargoc_flags \
        --features capi \
        -- -D warnings
    cd -
}

# Run action
case "$ACTION" in
    "build")
        do_build "$TARGET" "$OUT_DIR"
        ;;
    "test")
        do_test "$TARGET"
        ;;
    "clippy")
        do_clippy "$TARGET"
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
