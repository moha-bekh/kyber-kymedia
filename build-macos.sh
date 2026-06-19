#!/bin/bash
# Project Kyber: build-macos.sh
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

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)

# Architecture configuration
BUILD_ARCH=$(uname -m | cut -d. -f1)
HOST_ARCH="x86_64"

usage()
{
cat << EOF
usage: $0 -o <path> [options] [action]

Build kymedia application for macOS (kyaudioreg only - kyavserver not supported).

OPTIONS:
   -h            Show some help
   -k            Build Kyber dependencies
   -s            Skip native dependencies build
   -o <path>     Install the built binaries in the absolute path (required)
   -a <arch>     Use the specified arch (default: $HOST_ARCH)

ACTIONS:
   build                Build kyaudioreg (default)
   test                 Run tests on kyaudioreg
   clippy               Run clippy on kyaudioreg
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

get_cargo_target()
{
    case $1 in
        x86_64)
            echo "x86_64-apple-darwin"
        ;;
        arm64)
            echo "aarch64-apple-darwin"
        ;;
        *)
            echo "Unexpected arch $1"
            exit 1
        ;;
    esac
}

bison_version()
{
    "$1" --version 2>/dev/null | grep -oE '[0-9]+(\.[0-9]+)+' | head -n1
}

# VLC's JSON parser needs Bison >= 3; macOS ships 2.3, which meson silently picks
# then fails on. Prefer a modern bison on PATH, else a keg-only Homebrew one.
ensure_modern_bison()
{
    local ver
    ver=$(bison_version bison)
    if [ -n "$ver" ] && [ "${ver%%.*}" -ge 3 ] 2>/dev/null; then
        return 0
    fi

    if command -v brew >/dev/null 2>&1; then
        local prefix
        prefix=$(brew --prefix bison 2>/dev/null)
        if [ -x "$prefix/bin/bison" ]; then
            ver=$(bison_version "$prefix/bin/bison")
            if [ -n "$ver" ] && [ "${ver%%.*}" -ge 3 ] 2>/dev/null; then
                export PATH="$prefix/bin:$PATH"
                echo "Using Homebrew Bison $ver"
                return 0
            fi
        fi
    fi

    echo "Error: VLC needs Bison >= 3 (have: ${ver:-none}); macOS ships 2.3. Run: brew install bison" >&2
    exit 1
}

BUILD_DEPS=true
BUILD_KYBER_DEPS=false
ACTION="build"

while getopts "hkso:a:" OPTION
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
if [[ "$ACTION" == *"kyavserver"* ]]; then
    echo "Error: kyavserver is not supported on macOS"
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

# Setup environment
export PATH=$OUT_DIR/bin:$PATH
export PKG_CONFIG_ALLOW_CROSS=1
# PKG_CONFIG_LIBDIR must be set before meson setup so that FFmpeg's configure
# (which runs as an external_project at setup time) can find subproject-built
# libs like x264, opus, zlib via pkg-config. PKG_CONFIG_LIBDIR survives meson's
# internal PkgConfigInterface.setup_env() override (unlike PKG_CONFIG_PATH which
# gets replaced with [meson-uninstalled]).
export PKG_CONFIG_LIBDIR=$OUT_DIR/lib/pkgconfig
# Build dependencies (kyaudioreg + native deps via meson)
if [ "$BUILD_DEPS" = true ]; then
    # Always build server deps; add client deps (kyaudioreg + libvlc) only when KYBER_BUILD_CLIENT_DEPS=1.
    MESON_SETUP_ARGS=("--buildtype=release" "--wrap-mode=forcefallback" "-Dbuild_server=enabled")
    if [ "${KYBER_BUILD_CLIENT_DEPS:-0}" = "1" ]; then
        ensure_modern_bison
        ./build-kyaudioreg.sh -p macos -a "$HOST_ARCH" -o "$OUT_DIR"
        MESON_SETUP_ARGS+=("-Dbuild_client=enabled")
    else
        MESON_SETUP_ARGS+=("-Dbuild_client=disabled")
    fi

    # When cross-compiling
    if [ "$HOST_ARCH" != "$BUILD_ARCH" ]; then
        CPU_FAMILY=$([ "$HOST_ARCH" = "arm64" ] && echo "aarch64" || echo "$HOST_ARCH")
        sed \
            -e "s|@CPU_FAMILY@|${CPU_FAMILY}|g" \
            -e "s|@HOST_ARCH@|${HOST_ARCH}|g" \
            "$SCRIPT_DIR/macos-cross-meson-toolchain.ini.in" > "$SCRIPT_DIR/macos-cross-meson-toolchain.ini"
        MESON_SETUP_ARGS+=("--cross-file" "$SCRIPT_DIR/macos-cross-meson-toolchain.ini")
    fi

    if [ -d builddir-macos ]; then
        meson setup builddir-macos --prefix "$OUT_DIR" "${MESON_SETUP_ARGS[@]}" --reconfigure
    else
        meson setup builddir-macos --prefix "$OUT_DIR" "${MESON_SETUP_ARGS[@]}"
    fi
    meson compile -C builddir-macos -j$(sysctl -n hw.logicalcpu)
    meson install -C builddir-macos
else
    echo "Skip dependencies build"
fi

# Run action
case "$ACTION" in
    "build")
        echo "Nothing to do: kyaudioreg is built during the deps phase"
        ;;
    "test")
        echo "Test action not yet implemented for macOS"
        exit 1
        ;;
    "clippy")
        echo "Clippy action not yet implemented for macOS"
        exit 1
        ;;
    *)
        echo "Error: Unknown action '$ACTION'"
        usage
        exit 1
        ;;
esac
