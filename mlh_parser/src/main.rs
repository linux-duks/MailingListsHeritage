use env_logger::Env;

use mlh_parser::Result;
use mlh_parser::config;
use mlh_parser::start;

fn main() -> Result<()> {
    let env = Env::default().filter_or("RUST_LOG", "info");
    env_logger::init_from_env(env);

    log::info!("mlh_parser starting — build: {}", env!("CARGO_PKG_VERSION"));

    let mut app_config = match config::read_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!();
            eprintln!("Configuration options:");
            eprintln!("  - Config file:  parser_config.yaml (or similar)");
            eprintln!();
            eprintln!("Run with --help for more information.");
            std::process::exit(1);
        }
    };

    // Configure the global rayon thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(app_config.nthreads as usize)
        .build_global()
        .unwrap();

    start(&mut app_config)
}
