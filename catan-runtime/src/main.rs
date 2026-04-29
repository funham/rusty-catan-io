mod cli_child;
mod config;
mod host;

use std::path::PathBuf;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) == Some("cli-child") {
        let socket = arg_value(&args, "--socket").unwrap_or_else(|| {
            eprintln!("missing --socket");
            std::process::exit(2);
        });
        let role = arg_value(&args, "--role").unwrap_or_else(|| "unknown".to_owned());
        if let Err(err) = cli_child::run(&PathBuf::from(socket), &role) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

    env_logger::init();

    let config_path = args
        .get(1)
        .cloned()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("catan-runtime/data/configurations/cli_single.json"));

    let config = match host::load_config(&config_path) {
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

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == name).then(|| window[1].clone()))
}
