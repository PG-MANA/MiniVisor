#!/bin/bash

. tools/environment

cp $1 $DISK_IMG_DIR$BINARY_NAME

tools/create_boot_scr.sh
dtc -I dts -O dtb -o $DISK_IMG_DIR/DTB scripts/virt.dts
tools/create_disk.sh

tools/launch_qemu.sh
