use catan_runtime::host;

use std::path::PathBuf;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let config_path = args
        .get(1)
        .cloned()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("catan-runtime/data/configurations/bots3.json"));

    let config = match host::load_config(&config_path) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = catan_runtime::logging::init_host_logger(&config.logging) {
        eprintln!("{err}");
        std::process::exit(1);
    }

    if let Err(err) = host::run_match(config) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
