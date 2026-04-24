mod console_reporter;

use clap::Parser;
use console_reporter::ConsoleReporter;
use nlbn::{Cli, run_with_reporter};
use std::process;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .format(|buf, record| {
            use std::io::Write;
            writeln!(
                buf,
                "[{} {} nlbn] {}",
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                record.level(),
                record.args()
            )
        })
        .init();

    let args = Cli::parse();
    if args.debug {
        log::set_max_level(log::LevelFilter::Debug);
    }

    let reporter = Arc::new(ConsoleReporter::new());
    match run_with_reporter(args, reporter.clone()).await {
        Ok(Some(summary)) => reporter.report_summary(&summary),
        Ok(None) => reporter.report_no_work(),
        Err(error) => {
            eprintln!("Error: {}", error);
            process::exit(1);
        }
    }
}
