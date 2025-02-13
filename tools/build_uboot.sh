#!/bin/bash

. tools/environment

if [ -z $CROSS_COMPILE ]; then
    export CROSS_COMPILE=aarch64-linux-gnu-
fi

rm -rf $U_BOOT_DIR
git clone --depth=1 -b v2024.04 https://source.denx.de/u-boot/u-boot.git $U_BOOT_DIR

pushd $U_BOOT_DIR
make qemu_arm64_defconfig
make -j$(nproc)
cp u-boot.bin $BIN_DIR/u-boot.bin
popd
