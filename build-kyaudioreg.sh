#!/bin/bash
# Project Kyber: build-kyaudioreg.sh
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

SCRIPT_DIR=$(realpath $(dirname $0))
KYAUDIOREG_SRCDIR="$SCRIPT_DIR/kyaudioreg"

PLATFORM=""
HOST_ARCH=""
PREFIX=""
VLC_SRCDIR=""
ANDROID_API=21

usage()
{
cat << EOF
usage: $0 -p <platform> [options]

Build kyaudioreg for the target platform.

OPTIONS:
   -h              Show this help
   -p <platform>   Target platform: android, macos, win32, linux
   -a <arch>       Architecture (android: arm64|arm, macos: x86_64|arm64)
   -o <prefix>     Installation prefix (macos, win32, linux)
   -v <vlc-src>    VLC source directory (android, for contrib install path)
EOF
}

panic()
{
    echo "$1"
    exit 1
}

build_android()
{
    [ -n "$VLC_SRCDIR" ] || panic "VLC source directory required for android (-v)"
    [ -n "$ANDROID_NDK" ] || panic "ANDROID_NDK environment variable must be set"

    local CARGO_TARGET ANDROID_ARCH VLC_CONTRIB

    case $HOST_ARCH in
        arm | armv7a)
            CARGO_TARGET="armv7-linux-androideabi"
            ANDROID_ARCH="armv7a-linux-androideabi"
            VLC_CONTRIB="arm-linux-androideabi"
            export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="armv7a-linux-androideabi${ANDROID_API}-clang"
        ;;
        arm64)
            CARGO_TARGET="aarch64-linux-android"
            ANDROID_ARCH="aarch64-linux-android"
            VLC_CONTRIB="aarch64-linux-android"
            export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="aarch64-linux-android${ANDROID_API}-clang"
        ;;
        *) panic "Unexpected arch $HOST_ARCH" ;;
    esac

    export TARGET_AR=llvm-ar
    export TARGET_CC="${ANDROID_ARCH}${ANDROID_API}-clang"
    export RUSTFLAGS="-L native=${ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/${ANDROID_ARCH}/${ANDROID_API}"

    cd "$KYAUDIOREG_SRCDIR"
    cargo cinstall \
        --prefix="${VLC_SRCDIR}/contrib/${VLC_CONTRIB}" \
        --library-type=staticlib \
        --release \
        --target=$CARGO_TARGET
    cd -

    unset CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER
    unset CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER
    unset TARGET_AR
    unset TARGET_CC
    unset RUSTFLAGS
}

build_macos()
{
    [ -n "$PREFIX" ] || panic "Installation prefix required for macos (-o)"

    local CARGO_TARGET
    case $HOST_ARCH in
        x86_64) CARGO_TARGET="x86_64-apple-darwin" ;;
        arm64)  CARGO_TARGET="aarch64-apple-darwin" ;;
        *) panic "Unexpected arch $HOST_ARCH" ;;
    esac

    cd "$KYAUDIOREG_SRCDIR"
    cargo cinstall \
        --release \
        --library-type=cdylib \
        --prefix="$PREFIX" \
        --target="$CARGO_TARGET"
    cd -
}

build_win32()
{
    [ -n "$PREFIX" ] || panic "Installation prefix required for win32 (-o)"

    cd "$KYAUDIOREG_SRCDIR"
    cargo cinstall \
        --prefix="$PREFIX" \
        --release \
        --library-type=staticlib \
        --target=x86_64-pc-windows-gnu
    cd -
}

build_linux()
{
    [ -n "$PREFIX" ] || panic "Installation prefix required for linux (-o)"

    # Install as a shared lib; the VLC kyaudioreg plugin links it at runtime.
    # This also drops kyaudioreg.pc into $PREFIX/lib/pkgconfig so meson's
    # dependency('kyaudioreg') resolves and the plugin gets enabled.
    cd "$KYAUDIOREG_SRCDIR"
    cargo cinstall \
        --release \
        --library-type=cdylib \
        --prefix="$PREFIX"
    cd -
}

while getopts "hp:a:o:v:" OPTION
do
    case $OPTION in
        h) usage; exit 0 ;;
        p) PLATFORM=$OPTARG ;;
        a) HOST_ARCH=$OPTARG ;;
        o) PREFIX=$OPTARG ;;
        v) VLC_SRCDIR=$OPTARG ;;
        *) usage; exit 1 ;;
    esac
done

[ -n "$PLATFORM" ] || panic "Platform is required (-p)"

case $PLATFORM in
    android) build_android ;;
    macos)   build_macos ;;
    win32)   build_win32 ;;
    linux)   build_linux ;;
    *) panic "Unknown platform: $PLATFORM" ;;
esac
