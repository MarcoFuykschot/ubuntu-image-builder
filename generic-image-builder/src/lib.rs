use std::{
    fmt::Display, fs::File, path::{ Path, PathBuf}
};

use anyhow::{bail, ensure, Error};
use foyer_bytesize::ByteSize;
use serde::{Deserialize, Serialize};

mod shell_commands;
use shell_commands::*;

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

impl ToString for Distributions {
    fn to_string(&self) -> String {
        match self {
           Self::Jammy => String::from("jammy"),
           Self::Noble => String::from("noble")
        }
    }
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

#[derive(Debug,Clone,PartialEq)]
pub struct LoopDev(String);

impl LoopDev {
   pub fn boot_partition(&self) -> String {
     format!("{}p1",self.0)
   } 
   pub fn root_partition(&self) -> String {
     format!("{}p2",self.0)
   } 
}

impl Display for LoopDev {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug,Clone,PartialEq)]
pub enum ImageBuilderState {
    Phase0,
    Phase1(LoopDev),
    Phase2(LoopDev),
    Error
}

impl Display for ImageBuilderState {

    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
         match self {
            Self::Error => f.write_str("Error"),
            Self::Phase0 => f.write_str("PHASE0:"),
            Self::Phase1(dev) => f.write_fmt(format_args!("PHASE1:{}:",dev)),
            Self::Phase2(dev) => f.write_fmt(format_args!("PHASE2:{}:",dev)),
         }
    }
}

impl ImageBuilder {
    
    pub fn phase1(&self,phase:ImageBuilderState) -> Result<ImageBuilderState, Error> {

        ensure!( ImageBuilderState::Phase0 == phase  );

        let root_size = &self.image.size;
        let esp_size = ByteSize::mib(512);
        let disk_size = esp_size + *root_size;

        Self::log(&phase,"create diskimage");
        dd(&self.image.name, disk_size.as_u64())?;

        partition_disk(&self.image.name, (root_size.as_u64()/512)-2048-34)?;

        let loop_dev= create_loopdev(&self.image.name)?.first().cloned();
        let Some(loop_dev) = loop_dev.clone() else { bail!("loop device not valid"); };
        let loop_dev = LoopDev(loop_dev);

        println!("'{}'",loop_dev);
        println!("{}", status_loopdev(&loop_dev.to_string())? );
        println!("{:?}", status_fdisk(&loop_dev.to_string())? );

        mkfs_vfat( &loop_dev.boot_partition())?;
        mkfs_ext4( &loop_dev.root_partition())?;

        Ok( ImageBuilderState::Phase1(loop_dev))
    }

    fn log(phase:&ImageBuilderState,message:&str) {
        println!("{}:{}",phase,message);
    }

    pub fn phase2(&self,phase:ImageBuilderState) -> Result<ImageBuilderState,Error> {
        let ImageBuilderState::Phase1(loop_dev) = phase.clone() else { bail!("Invalid state {:?}",phase)};

        std::env::set_current_dir(&self.config.workdir)?;
        
        Self::log(&phase,"create and mount root");
        std::fs::create_dir("chroot")?;
        mount( &loop_dev.root_partition(),"chroot" )?; 

        Self::log(&phase,"create and mount boot/efi");
        std::fs::create_dir_all("chroot/boot/efi")?;
        mount( &loop_dev.boot_partition(),"chroot/boot/efi" )?;

        Self::log(&phase,"install debian bootstrap");
        
        println!("{:?}", bootstrap(&self.image.distro, "http:://archive.ubuntu.com/ubuntu")? );

        Ok(ImageBuilderState::Phase2(loop_dev))
    }

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

        Ok(ImageBuilderState::Phase0)
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

