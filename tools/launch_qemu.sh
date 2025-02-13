#!/bin/bash

. tools/environment

$QEMU   -M virt,gic-version=3,secure=off,virtualization=on \
        -smp 4 -bios $BIN_DIR/u-boot.bin -cpu cortex-a53 -m 2G \
        -nographic -device virtio-blk-device,drive=disk \
        -drive file=fat:rw:$DISK_IMG_DIR,format=raw,if=none,media=disk,id=disk
