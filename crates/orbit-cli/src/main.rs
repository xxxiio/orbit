use clap::Parser;
use orbit_cli::{Cli, CliError, render_error_json, run};

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(output) => println!("{output}"),
        Err(CliError::Diagnostics(report)) => {
            // The report is already rendered (human or JSON). Print it exactly
            // once and never decorate it with the `orbit:` error prefix.
            println!("{report}");
            std::process::exit(1);
        }
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
