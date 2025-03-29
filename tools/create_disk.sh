#!/bin/bash

. tools/environment

if [ ! -f $DISK_IMG ]; then
    dd if=/dev/zero of=$DISK_IMG  bs=1024 count=2048000
fi
echo -e "o\nn\np\n1\n2048\n\nt\nc\nw\n" | sudo fdisk $DISK_IMG || sudo rm -rf $DISK_IMG
sudo mkfs.vfat -F 32 --offset=2048 $DISK_IMG

rm -rf $DISK_MOUNT_DIR
mkdir -p $DISK_MOUNT_DIR

sudo mount -o loop,offset=$((2048 * 512)) $DISK_IMG $DISK_MOUNT_DIR
sudo cp -r $DISK_IMG_DIR* $DISK_MOUNT_DIR
sync
sudo umount $DISK_MOUNT_DIR
