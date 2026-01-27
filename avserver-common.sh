# Project Kyber: avserver-common.sh
# Copyright © 2022-2026 Kyber SAS
# SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0
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

#!/bin/bash
# Common build functions for all platforms
# Functions are prefixed by platform (linux_, win32_) to avoid conflicts

# Linux platform functions
avserver_linux_setup_env()
{
    local out_dir=$1
    export PATH=$out_dir/bin:$PATH
    export PKG_CONFIG_PATH=$out_dir/lib/pkgconfig:$out_dir/lib/x86_64-linux-gnu/pkgconfig
    export LD_LIBRARY_PATH=$out_dir/lib:$out_dir/lib/x86_64-linux-gnu
}

avserver_linux_do_build()
{
    local out_dir=$1

    # Build
    cargo build \
        -p avserver \
        --release \
        --bins \
        --examples

    # Install
    mkdir -p $out_dir/bin

    cp \
        target/release/avserver \
        $out_dir/bin
}

avserver_linux_do_clippy()
{
    cargo clippy -- -D warnings -A clippy::missing_safety_doc
}

avserver_linux_do_test()
{
    local out_dir=$1
    export LD_LIBRARY_PATH=$out_dir/lib:$out_dir/lib/x86_64-linux-gnu
    if [ -z $KYBER_TEST_NO_DEFAULT_FEATURES ]; then
        cargo test -p avserver
    else
        cargo test -p avserver --no-default-features
    fi
}

# Windows platform functions
avserver_win32_setup_env()
{
    local out_dir=$1
    export PATH=$out_dir/bin:$PATH
    export PKG_CONFIG_ALLOW_CROSS=1
    export PKG_CONFIG_LIBDIR=$out_dir/lib/pkgconfig
}

avserver_win32_do_build()
{
    local out_dir=$1
    local hide_console_feature=$2

    # Build
    cargo build \
        -p avserver \
        --target x86_64-pc-windows-gnu \
        --release \
        --bins \
        --examples \
        $hide_console_feature

    # Install
    mkdir -p $out_dir/bin

    cp \
        target/x86_64-pc-windows-gnu/release/avserver.exe \
        $out_dir/bin
}

avserver_win32_do_clippy()
{
    cargo clippy \
        -p avserver \
        --target x86_64-pc-windows-gnu \
        -- -D warnings -A clippy::missing_safety_doc
}
