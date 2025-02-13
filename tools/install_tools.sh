#!/bin/bash

SUDO=$(which sudo 2> /dev/null) || SUDO="" # For docker

$SUDO env DEBIAN_FRONTEND=noninteractive apt install -y \
    build-essential make bison bc flex gcc-aarch64-linux-gnu \
    libssl-dev device-tree-compiler \
    wget cpio unzip rsync \
    sudo fdisk dosfstools file # For docker
