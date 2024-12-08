use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::ensure;
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

impl ImageBuilder {
    pub fn create(configpath: &Path) -> Result<Self, anyhow::Error> {
        let configfile = File::open(configpath)?;

        let config: Self = serde_yml::from_reader(configfile)?;
        config.is_valid()?;

        Self::check_required_tools()?;

        Ok(config)
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
        let tools = vec!["sfdisk", "debootstrap", "mount","dd","losetup","mkfs.vfat","mkfs.ext4","chroot","apt"];

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
