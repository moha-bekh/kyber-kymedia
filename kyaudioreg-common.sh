#!/bin/bash
# Common build functions for all platforms
# Functions are prefixed by platform (linux_, macos_, win32_, wasm_) to avoid conflicts

# Linux platform functions
kyaudioreg_linux_do_build()
{
    local out_dir=$1
    cargo cinstall \
        -p kyaudioreg \
        --release \
        --prefix="$out_dir"
}

kyaudioreg_linux_do_test()
{
    cargo test \
        -p kyaudioreg \
        --release
}

kyaudioreg_linux_do_clippy()
{
    cargo clippy \
        -p kyaudioreg \
        --features capi \
        -- -D warnings
}

# macOS platform functions
kyaudioreg_macos_do_build()
{
    local cargo_target=$1
    local out_dir=$2

    cargo cinstall \
        -p kyaudioreg \
        --prefix="$out_dir" \
        --target="$cargo_target" \
        --release
}

kyaudioreg_macos_do_test()
{
    local cargo_target=$1

    cargo test \
        -p kyaudioreg \
        --target="$cargo_target" \
        --release \
        --features capi
}

kyaudioreg_macos_do_clippy()
{
    local cargo_target=$1

    cargo clippy \
        -p kyaudioreg \
        --target="$cargo_target" \
        --features capi \
        -- -D warnings
}

# Windows platform functions
kyaudioreg_win32_do_build()
{
    local out_dir=$1

    cargo cinstall \
        -p kyaudioreg \
        --release \
        --prefix="$out_dir" \
        --target=x86_64-pc-windows-gnu \
        --dlltool=x86_64-w64-mingw32-dlltool
}

kyaudioreg_win32_do_clippy()
{
    cargo clippy \
        -p kyaudioreg \
        --target x86_64-pc-windows-gnu \
        --features capi \
        -- -D warnings
}

# WASM platform functions
kyaudioreg_wasm_do_build()
{
    local web_features=$1

    cargo build \
        -p kyaudioreg \
        --target wasm32-unknown-unknown \
        --features "$web_features"
}

kyaudioreg_wasm_do_clippy()
{
    local web_features=$1

    cargo clippy \
        -p kyaudioreg \
        --target wasm32-unknown-unknown \
        --features "$web_features" \
        -- -D warnings
}

# Android platform functions
kyaudioreg_android_do_build()
{
    local out_dir=$1
    local arch=$2

    ANDROID_API=21

    case $arch in
        armv7a)
            CARGO_TARGET="armv7-linux-androideabi"
            ANDROID_ARCH="armv7a-linux-androideabi"
            VLC_CONTRIB="arm-linux-androideabi"

            export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="armv7a-linux-androideabi$ANDROID_API-clang"
        ;;
        arm64)
            CARGO_TARGET="aarch64-linux-android"
            ANDROID_ARCH="aarch64-linux-android"
            VLC_CONTRIB="aarch64-linux-android"

            export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="aarch64-linux-android$ANDROID_API-clang"
        ;;
        *)
            echo "Unexpected arch $HOST_ARCH"
            exit 1
        ;;
    esac

    export TARGET_AR=llvm-ar
    export TARGET_CC=$ANDROID_ARCH$ANDROID_API-clang
    export RUSTFLAGS="-L native=$ANDROID_NDK/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/$ANDROID_ARCH/$ANDROID_API"

    cargo cinstall \
        -p kyaudioreg \
        --prefix="$out_dir" \
        --library-type=staticlib \
        --release \
        --target=$CARGO_TARGET

    unset CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER
    unset CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER
    unset TARGET_AR
    unset TARGET_CC
    unset RUSTFLAGS
}

# iOS/tvOS/visionOS platform functions
kyaudioreg_ios_get_cargo_target()
{
    case $1 in
        ios)
            echo "aarch64-apple-ios"
        ;;
        ios-sim)
            echo "aarch64-apple-ios-sim"
        ;;
        tvos)
            echo "aarch64-apple-tvos"
        ;;
        tvos-sim)
            echo "aarch64-apple-tvos-sim"
        ;;
        visionos)
            echo "aarch64-apple-visionos"
        ;;
        visionos-sim)
            echo "aarch64-apple-visionos-sim"
        ;;
        *)
            echo "Unexpected target $1"
            exit 1
        ;;
    esac
}

kyaudioreg_ios_get_cargoc_flags()
{
    case $1 in
        ios|ios-sim)
            echo ""
        ;;
        tvos|tvos-sim)
            echo "-Zbuild-std=std,panic_abort"
        ;;
        visionos|visionos-sim)
            echo "-Zbuild-std=std,panic_abort"
        ;;
        *)
            echo "Unexpected target $1"
            exit 1
        ;;
    esac
}

kyaudioreg_ios_do_build()
{
    local target=$1
    local out_dir=$2

    local cargo_target=`kyaudioreg_ios_get_cargo_target $target`
    local cargoc_flags=`kyaudioreg_ios_get_cargoc_flags $target`

    cargo cinstall \
        -p kyaudioreg \
        --target=$cargo_target \
        $cargoc_flags \
        --release \
        --prefix=$out_dir
}

kyaudioreg_ios_do_test()
{
    local target=$1

    local cargo_target=`kyaudioreg_ios_get_cargo_target $target`
    local cargoc_flags=`kyaudioreg_ios_get_cargoc_flags $target`

    cargo test \
        -p kyaudioreg \
        --target=$cargo_target \
        $cargoc_flags \
        --release \
        --features capi
}

kyaudioreg_ios_do_clippy()
{
    local target=$1

    local cargo_target=`kyaudioreg_ios_get_cargo_target $target`
    local cargoc_flags=`kyaudioreg_ios_get_cargoc_flags $target`

    cargo clippy \
        -p kyaudioreg \
        --target=$cargo_target \
        $cargoc_flags \
        --features capi \
        -- -D warnings
}
