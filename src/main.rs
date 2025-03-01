mod config;
mod crawler;
mod meilisearch;
mod scraper;

use crate::config::CrawlerConfig;
use crate::crawler::Crawler;
use clap::{Parser, Subcommand};
use env_logger::Env;
use std::fs;
use std::time::Instant;

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

        /// Dry run: validate config only, don't crawl
        #[arg(short, long, default_value_t = false)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        Commands::Crawl { config, dry_run } => {
            println!("Loading configuration from: {}", config);
            let start = Instant::now();

            let config_content = fs::read_to_string(&config)
                .map_err(|e| format!("Failed to read config file '{}': {}", config, e))?;

            let crawler_config: CrawlerConfig = serde_json::from_str(&config_content)
                .map_err(|e| format!("Failed to parse config file '{}': {}", config, e))?;

            // Show config summary
            println!("Crawl configuration:");
            println!("  URL: {}", crawler_config.url);
            println!("  Mode: {:?}", crawler_config.mode);
            println!("  Include subdomains: {}", crawler_config.subdomains);
            println!("  Max depth: {:?}", crawler_config.crawl_depth);
            println!("  Delay: {:?}ms", crawler_config.crawl_delay);
            println!("  Workers: {:?}", crawler_config.concurrency);

            if dry_run {
                println!("Dry run completed - configuration is valid");
                return Ok(());
            }

            let crawler = Crawler::new(crawler_config.url.clone(), crawler_config);

            log::info!("Starting crawler...");
            crawler.crawl().await;

            log::info!("Crawl completed successfully");
            Ok(())
        }
    }
}
