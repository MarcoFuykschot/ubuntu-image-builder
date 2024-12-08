use clap::Parser;
use generic_image_builder::ImageBuilder;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    configpath: PathBuf,
}

pub fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    println!("{:?}", ImageBuilder::create(&args.configpath)?);

    Ok(())
}
