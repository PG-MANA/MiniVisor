#!/bin/bash

. tools/environment

cp $1 $DISK_IMG_DIR$BINARY_NAME

tools/create_boot_scr.sh
tools/launch_qemu.sh
