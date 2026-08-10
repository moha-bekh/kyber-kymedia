#!/bin/bash
# Project Kyber: build-wasm.sh
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

usage()
{
cat << EOF
usage: $0 [options] [action]

Build kymedia application for WASM (kyaudioreg only - kyavserver not supported).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies
   -f <features> Specify features

ACTIONS:
   build                Build kyaudioreg for WASM (default)
   clippy               Run clippy on kyaudioreg for WASM
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

BUILD_KYBER_DEPS=false
ACTION="build"
WEB_FEATURES=""

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
if [[ "$ACTION" == *"kyavserver"* ]]; then
    echo "Error: kyavserver is not supported for WASM"
    usage
    exit 1
fi

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
    echo 'log4ky = { path = "kyutil/log4ky" }' >> .cargo/config.toml
    echo 'libkypc = { path = "kyutil/libkypc" }' >> .cargo/config.toml
fi

# WASM build functions
do_build() {
    local web_features=$1

    cd kyaudioreg
    cargo build \
        --target wasm32-unknown-unknown \
        --release \
        --features "$web_features"
    cd -
}

do_clippy() {
    local web_features=$1

    cd kyaudioreg
    cargo clippy \
        --target wasm32-unknown-unknown \
        --release \
        --features "$web_features" \
        -- -D warnings
    cd -
}

# Run action
case "$ACTION" in
    "build")
        do_build "$WEB_FEATURES"
        ;;
    "clippy")
        do_clippy "$WEB_FEATURES"
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
