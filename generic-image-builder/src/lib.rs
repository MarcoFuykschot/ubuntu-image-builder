use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Error};
use foyer_bytesize::ByteSize;
use serde::{Deserialize, Serialize};
use shellfn::shell;

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct ImageBuilder {
    config: BuilderConfig,
    image: ImageConfig,
    content: ImageContent,
}

#[derive(Serialize, Debug, Clone, Deserialize)]
struct BuilderConfig {
    workdir: PathBuf,
}

#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Distributions {
    Noble,
    Jammy,
}

#[derive(Serialize, Debug, Clone, Deserialize)]
struct ImageConfig {
    name: String,
    distro: Distributions,
    size: ByteSize,
}

#[derive(Serialize, Debug, Clone, Deserialize)]
struct ImageContent {
    base: PathBuf,
    apt_packages: Option<Vec<String>>,
    local_package_dir: Option<PathBuf>,
    local_packages: Option<Vec<String>>,
    scripts: Option<Vec<String>>,
}

pub struct ImageBuilderState {
    config: ImageBuilder,
    loop_dev: Option<String>,
    directory_stack: Vec<PathBuf>,
}

impl ImageBuilderState {
    pub fn phase1(&mut self) -> Result<&mut ImageBuilderState, Error> {
        let root_size = &self.config.image.size;
        let esp_size = ByteSize::mib(512);
        let disk_size = esp_size + *root_size;

        dd(&self.config.image.name, disk_size.as_u64())?;

        partition_disk(&self.config.image.name, (root_size.as_u64()/512)-2048-34)?;

        self.loop_dev= create_loopdev(&self.config.image.name)?.first().cloned();

        let Some(loop_dev) = self.loop_dev.clone() else { bail!("loop device not valid"); };

        println!("'{}'",loop_dev);
        println!("{}", status_loopdev(&loop_dev)? );
        println!("{:?}", status_fdisk(&loop_dev)? );

        mkfs_vfat(format!("{}p1",loop_dev).as_str())?;
        mkfs_ext4(format!("{}p2",loop_dev).as_str())?;

        Ok(self)
    }
}

impl ImageBuilder {
    pub fn create(configpath: &Path) -> Result<Self, anyhow::Error> {
        let configfile = File::open(configpath)?;

        let config: Self = serde_yml::from_reader(configfile)?;
        config.is_valid()?;

        Self::check_required_tools()?;

        Ok(config)
    }

    pub fn phase0(&self) -> Result<ImageBuilderState, anyhow::Error> {
        std::fs::create_dir_all(&self.config.workdir)?;
        std::env::set_current_dir(&self.config.workdir)?;

        Ok(ImageBuilderState {
            config: self.clone(),
            loop_dev:None,
            directory_stack: vec![std::env::current_dir()?],
        })
    }

    fn is_valid(&self) -> Result<(), anyhow::Error> {
        ensure!(
            !self.config.workdir.exists(),
            "workdir in config already exists {:?}",
            self.config.workdir
        );
        ensure!(
            self.content.base.exists(),
            "content directory {:?} should exist",
            self.content.base
        );
        ensure!(
            self.content
                .local_package_dir
                .clone()
                .is_some_and(|path| path.exists()),
            "local package directory {:?} should exist",
            self.content.local_package_dir
        );

        Ok(())
    }

    fn check_required_tools() -> Result<(), anyhow::Error> {
        let tools = vec![
            "sfdisk",
            "debootstrap",
            "mount",
            "dd",
            "losetup",
            "mkfs.vfat",
            "mkfs.ext4",
            "chroot",
            "apt",
        ];

        tools.iter().try_for_each(|tool| {
            ensure!(command_exists(tool).is_ok(), "{} should be installed", tool);
            Ok(())
        })?;

        Ok(())
    }
}

#[shell]
fn command_exists(name: &str) -> Result<String, anyhow::Error> {
    "command -v $NAME"
}

#[shell]
fn dd(image_name: &str, blocks: u64) -> Result<String, anyhow::Error> {
    "dd if=/dev/zero of=$IMAGE_NAME bs=1 count=0 seek=$BLOCKS"
}


#[shell]
fn partition_disk(image_name: &str,sectors:u64) -> Result<String,anyhow::Error> {
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
fn create_loopdev(image_name: &str) -> Result<Vec<String>,anyhow::Error> {
  "losetup --show -fP $IMAGE_NAME"
}

#[shell]
fn status_loopdev(loopdev:&str) -> Result<String,anyhow::Error> {
  "losetup -l $LOOPDEV"
}

#[shell]
fn status_fdisk(loopdev:&str) -> Result<Vec<String>,anyhow::Error> {
  "fdisk -l $LOOPDEV"
}

#[shell]
fn mkfs_vfat(loop_partition:&str) -> Result<Vec<String>,anyhow::Error> {
   "mkfs.vfat $LOOP_PARTITION"
}

#[shell]
fn mkfs_ext4(loop_partition:&str) -> Result<Vec<String>,anyhow::Error> {
  "mkfs.ext4 $LOOP_PARTITION"
}