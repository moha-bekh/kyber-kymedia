#!/bin/bash
# Project Kyber: build-linux.sh
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

BASE_DIR=$(realpath $(dirname $0))

usage()
{
cat << EOF
usage: $0 -o <path> [options] [action]

Build kymedia application (kyavserver and kyaudioreg).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies (for kyavserver)
   -s            Skip native dependencies build (for kyavserver)
   -d            Build native dependencies (contrib) only, then exit
   -o <path>     Install the built binaries in the absolute path (required)

ACTIONS:
   build                Build kyavserver (default)
   test                 Run tests on kyavserver
   clippy               Run clippy on kyavserver
EOF
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

BUILD_DEPS=true
BUILD_KYBER_DEPS=false
DEPS_ONLY=false
ACTION="build"

while getopts "hksdo:" OPTION
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
        d)
            DEPS_ONLY=true
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

# Build dependencies (contrib builds kyaudioreg + native deps)
if [ "$BUILD_DEPS" = true ]; then
    ./contrib/build-linux.sh -o $OUT_DIR
else
    echo "Skip dependencies build"
fi

# If deps only mode, exit here
if [ "$DEPS_ONLY" = true ]; then
    echo "Dependencies built successfully"
    exit 0
fi

# Setup environment for kyavserver
export PATH=$OUT_DIR/bin:$PATH
export PKG_CONFIG_PATH=$OUT_DIR/lib/pkgconfig:$OUT_DIR/lib/x86_64-linux-gnu/pkgconfig:$OUT_DIR/lib64/pkgconfig
export LD_LIBRARY_PATH=$OUT_DIR/lib:$OUT_DIR/lib/x86_64-linux-gnu:$OUT_DIR/lib64

# Avserver build functions
kyavserver_linux_do_build() {
    local out_dir=$1

    # Build
    cargo build \
        -p kyavserver \
        --release \
        --bins \
        --examples

    # Install
    mkdir -p $out_dir/bin

    cp \
        target/release/kyavserver \
        $out_dir/bin
}

kyavserver_linux_do_clippy() {
    cargo clippy -- -D warnings -A clippy::missing_safety_doc
}

kyavserver_linux_do_test() {
    local out_dir=$1
    export LD_LIBRARY_PATH=$out_dir/lib:$out_dir/lib/x86_64-linux-gnu:$out_dir/lib64
    if [ -z $KYBER_TEST_NO_DEFAULT_FEATURES ]; then
        cargo test -p kyavserver
    else
        cargo test -p kyavserver --no-default-features
    fi
}

# Run action
case "$ACTION" in
    "build")
        kyavserver_linux_do_build "$OUT_DIR"
        ;;
    "test")
        kyavserver_linux_do_test "$OUT_DIR"
        ;;
    "clippy")
        kyavserver_linux_do_clippy
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
