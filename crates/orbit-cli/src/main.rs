use clap::Parser;
use orbit_cli::{Cli, render_error_json, run};

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            if json {
                eprintln!("{}", render_error_json(&error));
            } else {
                eprintln!("orbit: {error}");
            }
            std::process::exit(error.exit_code());
        }
    }
}
