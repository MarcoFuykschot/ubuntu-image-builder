#!/bin/bash

set -e

CONFIG_FILE="$(cat $1 | yq)"
echo $CONFIG_FILE

function phase1() {
    WORKDIR=$1
    IMAGE_NAME=$2
    IMAGE_SIZE=$3

    mkdir $WORKDIR
    pushd $WORKDIR &>$WORKDIR/phase1.log

    echo "PHASE-1: create image"
    BLOCKS=$(numfmt --from=iec $IMAGE_SIZE)
    ESP=$(numfmt --from=iec 512M)
    BLOCKS=$(($ESP + $BLOCKS))
    dd if=/dev/zero of=$IMAGE_NAME bs=1 count=0 seek=$BLOCKS &>>phase1.log

    echo "PHASE-1: create partitions"
    cat <<EOF | sfdisk $IMAGE_NAME &>>phase1.log
label: gpt
unit: sectors
first-lba: 2048
sector-size: 512

start=2048, size=512M , type=U
            size=$PART_SIZE, type=L
EOF

    export LOOP=$(losetup --show -fP $IMAGE_NAME)
    losetup -l $LOOP &>>phase1.log
    fdisk -l $LOOP &>>phase1.log

    echo "PHASE-1: create filesystems"
    mkfs.vfat $(echo $LOOP"p1") &>>phase1.log
    mkfs.ext4 $(echo $LOOP"p2") &>>phase1.log
    popd &>>phase1.log
}

function phase2() {
    WORKDIR=$1
    LOOP=$2
    DISTRO="noble"
    MIRROR="http://nl.archive.ubuntu.com/ubuntu/"
    P1=$(echo $LOOP"p1")
    P2=$(echo $LOOP"p2")

    pushd $WORKDIR &>$WORKDIR/phase2.log

    echo "PHASE-2: Setup default mounts"
    mkdir chroot
    mount $P2 chroot
    mkdir -p chroot/boot
    mkdir -p chroot/boot/efi
    mount $P1 chroot/boot/efi

    echo "PHASE-2: Setup default content"
    debootstrap \
        --arch=amd64 \
        --variant=minbase \
        --components "main,universe" \
        --include "ca-certificates,cron,iptables,isc-dhcp-client,libnss-myhostname,ntp,ntpdate,rsyslog,ssh,sudo,dialog,whiptail,man-db,curl,dosfstools,e2fsck-static" \
        $DISTRO \
        chroot \
        $MIRROR &>>phase2.log

    rm -rf chroot/var/cache/apt/*
    mount --bind /dev chroot/dev
    mount --bind /run chroot/run

    mkdir -p $WORKDIR/chroot/install
    popd &>>phase2.log

    echo "PHASE-2: Setup custom content"
    CONTENT_DIR=$(echo $CONFIG_FILE | jq -r '.content.base')
    cp -vr $CONTENT_DIR $WORKDIR/chroot/install

}

function phase3() {
    WORKDIR=$1
    export LOOP=$2
    export HOSTNAME=testhost
    export DISTRO=noble

    P2=$(echo $LOOP"p2")

    export $(blkid -o export $P2 | grep UUID)
    export P2_UUID=$UUID

    echo "PHASE-3: create install and config files in image"
    cat <<EOF >$WORKDIR/chroot/etc/apt/sources.list
deb http://archive.ubuntu.com/ubuntu/ $DISTRO main universe 
deb-src http://archive.ubuntu.com/ubuntu/ $DISTRO main universe 

deb http://archive.ubuntu.com/ubuntu/ $DISTRO-security main universe 
deb-src http://archive.ubuntu.com/ubuntu/ $DISTRO-security main universe 

deb http://archive.ubuntu.com/ubuntu/ $DISTRO-updates main universe 
deb-src http://archive.ubuntu.com/ubuntu/ $DISTRO-updates main universe 
EOF

    cat <<EOF >$WORKDIR/chroot/etc/fstab
# /etc/fstab: static file system information.
# <file system>         <mount point>   <type>  <options>                       <dump>  <pass>
/dev/disk/by-uuid/$P2_UUID      /               ext4    errors=remount-ro               0       1
EOF

    cat <<EOF >$WORKDIR/chroot/install/phase3a.sh
mount  none -t proc /proc
mount none -t sysfs /sys
mount none -t devpts /dev/pts
mount none -t tmpfs /var/cache/apt
mount none -t tmpfs /var/lib/apt

mkdir -p /usr/lib/firmware
mount none -t tmpfs /usr/lib/firmware
export HOME=/root
export LC_ALL=C
echo "$HOSTNAME" > /etc/hostname
exit
EOF

    cat <<EOF >$WORKDIR/chroot/install/phase3b.sh
apt update
apt -qq install -y systemd-sysv 
dbus-uuidgen > /etc/machine-id
ln -fs /etc/machine-id /var/lib/dbus/machine-id
dpkg-divert --local --rename --add /sbin/initctl
ln -s /bin/true /sbin/initctl

apt -qq --no-install-recommends install -y \
    ubuntu-server-minimal \
    linux-image-generic  \
    zstd \
    grub-efi-amd64
exit
EOF

    cat <<EOF >$WORKDIR/chroot/install/phase3c.sh
grub-install --target=x86_64-efi $LOOP --efi-directory=/boot/efi --boot-directory=/boot
update-grub
exit
EOF

    cat <<EOF >$WORKDIR/chroot/install/phase3d.sh
    apt -y install $(echo $CONFIG_FILE | jq -r '.content.apt_packages.[]|. + " "')
    pushd /install/content/packages &>/dev/null
    $(echo $CONFIG_FILE | jq -r '.content.local_packages.[]|"apt -y install ./" + .')
    popd &>/dev/null
    pushd /install/content/scripts &>/dev/null
    chmod -R a+x *.sh
    popd &>/dev/null
    $(echo $CONFIG_FILE | jq -r '.content.scripts.[]|"/install/content/scripts/"+.')
EOF

    cat <<EOF >$WORKDIR/chroot/install/phase4a.sh
apt -qq -y upgrade

echo 'root:root' | chpasswd

truncate -s 0 /etc/machine-id
rm /sbin/initctl
dpkg-divert --rename --remove /sbin/initctl
apt-get clean
rm -rf /tmp/* ~/.bash_history
rm -rf /phase3
export HISTSIZE=0
exit
EOF

    echo "PHASE-3a: setup mount points"
    chroot $WORKDIR/chroot bash -e /install/phase3a.sh &>$WORKDIR/phase3a.log
    echo "PHASE-3b: install default ubuntu packages and kernel"
    chroot $WORKDIR/chroot bash -e /install/phase3b.sh &>$WORKDIR/phase3b.log
    echo "PHASE-3c: setup boot partition"
    chroot $WORKDIR/chroot bash -e /install/phase3c.sh &>$WORKDIR/phase3c.log
    echo "PHASE-3d: custom packages and scripts from content"
    chroot $WORKDIR/chroot bash -e /install/phase3d.sh
    echo "PHASE-4a: upgrade all and cleanup in image"
    chroot $WORKDIR/chroot bash -e /install/phase4a.sh &>$WORKDIR/phase4a.log

}

function phase4() {
    WORKDIR=$1
    TYPE=$2
    echo "PHASE-4b: cleanup build $TYPE"

    MOUNTS=(dev/pts var/lib/apt usr/lib/firmware dev proc sys run boot/efi var/cache/apt)
    for MOUNT in ${MOUNTS[@]}; do
        if [ "$TYPE" == "normal" ]; then
            umount $WORKDIR/chroot/$MOUNT
        else
            umount $WORKDIR/chroot/$MOUNT || continue
        fi
    done

    if [ "$TYPE" == "normal" ]; then
        umount $WORKDIR/chroot
        P2=$(echo $LOOP"p2")
        e2fsck -f -y $P2
        resize2fs -M $P2
        losetup -D
    else
        umount $WORKDIR/chroot || true
        losetup -D || true
    fi

}

apt install debootstrap yq

WORKDIR="$(echo $CONFIG_FILE | jq -r '.config.workdir')"
IMAGE_NAME=$(echo $CONFIG_FILE | jq -r '.image.name')
SIZE=$(echo $CONFIG_FILE | jq -r '.image.size')

trap "phase4 $WORKDIR error" ERR
trap "phase4 $WORKDIR error" EXIT

phase1 $WORKDIR $IMAGE_NAME $SIZE
phase2 $WORKDIR $LOOP
phase3 $WORKDIR $LOOP

trap - ERR
trap - EXIT
phase4 $WORKDIR normal
