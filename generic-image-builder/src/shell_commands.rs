use std::any;

use shellfn::shell;

#[shell]
pub (crate) fn command_exists(name: &str) -> Result<String, anyhow::Error> {
    "command -v $NAME"
}

#[shell]
pub (crate) fn dd(image_name: &str, blocks: u64) -> Result<String, anyhow::Error> {
    "dd if=/dev/zero of=$IMAGE_NAME bs=1 count=0 seek=$BLOCKS"
}


#[shell]
pub (crate) fn partition_disk(image_name: &str,sectors:u64) -> Result<String,anyhow::Error> {
 r#"
   cat <<EOF | sfdisk $IMAGE_NAME
label: gpt
unit: sectors
first-lba: 2048
sector-size: 512

2048 +512M U
- $SECTORS L
EOF"#
}

#[shell]
pub (crate) fn create_loopdev(image_name: &str) -> Result<Vec<String>,anyhow::Error> {
  "losetup --show -fP $IMAGE_NAME"
}

#[shell]
pub (crate) fn status_loopdev(loopdev:&str) -> Result<String,anyhow::Error> {
  "losetup -l $LOOPDEV"
}

#[shell]
pub (crate) fn status_fdisk(loopdev:&str) -> Result<Vec<String>,anyhow::Error> {
  "fdisk -l $LOOPDEV"
}

#[shell]
pub (crate) fn mkfs_vfat(loop_partition:&str) -> Result<Vec<String>,anyhow::Error> {
   "mkfs.vfat $LOOP_PARTITION"
}

#[shell]
pub (crate) fn mkfs_ext4(loop_partition:&str) -> Result<Vec<String>,anyhow::Error> {
  "mkfs.ext4 $LOOP_PARTITION"
}

#[shell]
pub (crate) fn mount(dev:&str,mount_path:&str) -> Result<String,anyhow::Error> {
  "mount $DEV $MOUNT_PATH"
}

use crate::Distributions;

#[shell]
pub(crate) fn bootstrap(distro:&Distributions,mirror:&str) -> Result<Vec<String>,anyhow::Error> {
  r#" debootstrap --arch=amd64 --variant=minbase --components "main,universe" --include "ca-certificates,cron,iptables,isc-dhcp-client,libnss-myhostname,ntp,ntpdate,rsyslog,ssh,sudo,dialog,whiptail,man-db,curl,dosfstools,e2fsck-static"  $DISTRO  chroot  $MIRROR "#
}