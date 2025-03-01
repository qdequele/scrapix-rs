// Scrapix Crawler
//
// This module implements a simple web crawler using the spider crate.
// It provides functionality for crawling websites, collecting and formatting URLs.

use crate::config::CrawlerConfig;
use crate::scraper::{Scraper, ScraperResult};

use dashmap::DashMap;
use spider::website::Website;
use spider::CaseInsensitiveString;
use std::error::Error;

#[derive(Clone)]
pub struct Crawler {
    url: String,
    config: CrawlerConfig,
}

impl Crawler {
    pub fn new(url: String, config: CrawlerConfig) -> Self {
        Self { url, config }
    }

    /// Crawls a website and collects URLs after removing query parameters.
    ///
    /// # Returns
    /// Returns `Ok(Vec<String>)` containing all crawled URLs if successful, or an error.
    pub async fn crawl(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::info!("Setting up crawler for {}", self.url);

        // Create the website with basic settings
        let mut website = Website::new(&self.url);

        // Configure website crawler
        website.with_respect_robots_txt(true);
        website.with_user_agent(Some("Scrapix Bot"));
        website.with_return_page_links(true);
        website.with_normalize(true);

        // Set up callback to remove query parameters from URLs
        website.on_link_find_callback = Some(|url, source_url| {
            // Simple approach to remove query parameters
            // Split the URL at '?' and only keep the part before it
            let url_str = url.to_string();
            let clean_url = match url_str.find('?') {
                Some(pos) => CaseInsensitiveString::new(&url_str[..pos]),
                None => url,
            };

            // Return the modified URL
            (clean_url, source_url)
        });

        // Use a moderate channel capacity (100 is usually a good balance)
        // This allows enough buffering without causing the crawler to hang
        let mut rx2 = website.subscribe(888).unwrap();

        let config_clone = self.config.clone();
        let join_handle = tokio::spawn(async move {
            let scraper = Scraper::new(config_clone);
            let results = DashMap::new();

            // Process pages until we receive an error (typically when the channel is closed)
            loop {
                match rx2.recv().await {
                    Ok(res) => {
                        // Extract URL once to avoid borrowing res multiple times
                        let url = res.get_url().to_string();

                        if !results.contains_key(&url) {
                            // Scrape the content of the page with scraper module
                            let content = scraper.scrape_page(&res);
                            results.insert(url, content);
                        }
                    }
                    Err(_) => {
                        log::info!("Channel closed, processing complete");
                        break;
                    }
                }
            }

            results.into_iter().map(|(_, v)| v).collect()
        });

        // Build the website
        let mut website = website
            .build()
            .map_err(|e| format!("Failed to build website crawler: {}", e))?;

        log::info!("Starting crawl for {}", self.url);

        // Start the crawl
        website.crawl().await;

        // Close the subscription to signal the processor task to finish
        // website.unsubscribe();

        // Get all visited links
        let links = website.get_all_links_visited().await;
        log::info!("Crawling completed, visited {} pages", links.len());

        // Now that both the crawler and processor are done, await the join handle
        let results: Vec<ScraperResult> = join_handle.await.unwrap();
        log::info!("Scraped {} pages", results.len());
        Ok(())
    }
}
