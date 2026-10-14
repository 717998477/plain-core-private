use anyhow::Result;

mod bootstrap;
mod cli;
mod version;
mod web_bundle;

fn main() -> Result<()> {
    bootstrap::run()
}
