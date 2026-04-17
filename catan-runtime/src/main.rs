mod ascii_display;
mod config;
mod display;
mod host;

use std::path::PathBuf;

fn main() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("catan-runtime/data/configurations/cli_single.json"));

    let config = match host::load_config(&path) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = host::run_match(config) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
