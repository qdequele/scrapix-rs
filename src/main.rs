mod config;
mod crawler;
mod meilisearch;
mod scraper;

use crate::config::CrawlerConfig;
use crate::crawler::Crawler;
use clap::{Parser, Subcommand};
use env_logger::Env;
use std::fs;
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the web server
    Serve {
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        address: String,
    },
    /// Run crawler from CLI
    Crawl {
        #[arg(short, long)]
        config: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let env = Env::default()
        .filter_or("RUST_LOG", "info")
        .write_style_or("RUST_LOG_STYLE", "always");

    env_logger::init_from_env(env);

    match cli.command {
        Commands::Serve { address } => {
            println!("Starting server on {}", address);
            // TODO: Implement server
            Ok(())
        }
        Commands::Crawl { config } => {
            let config_content = fs::read_to_string(&config)
                .map_err(|e| format!("Failed to read config file: {}", e))?;

            let config: CrawlerConfig = serde_json::from_str(&config_content)
                .map_err(|e| format!("Failed to parse config file: {}", e))?;

            let crawler = Crawler::new(config);
            crawler.crawl().await?;

            Ok(())
        }
    }
}
