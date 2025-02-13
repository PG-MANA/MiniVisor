#!/bin/bash

. tools/environment

$U_BOOT_DIR/tools/mkimage -A arm64 -T script -C none -d scripts/boot.txt $DISK_IMG_DIR/boot.scr
