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
use std::sync::Arc;

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
        let mut rx2 = website.subscribe(16).unwrap();

        let scraper = Scraper::new(self.config.clone());
        let results = Arc::new(DashMap::new());
        tokio::spawn(async move {
            while let Ok(page) = rx2.recv().await {
                // Clone the resources for this task
                let scraper_clone = scraper.clone();
                let results_clone = Arc::clone(&results);

                tokio::spawn(async move {
                    let url = page.get_url().to_string();

                    if !results_clone.contains_key(&url) {
                        // Scrape the content of the page with scraper module
                        let content = scraper_clone.scrape_page(&page);
                        results_clone.insert(url, content);
                    }
                });
            }
            log::info!("Scraped {} pages", results.len());
        });

        // Build the website
        let mut website = website
            .build()
            .map_err(|e| format!("Failed to build website crawler: {}", e))?;

        log::info!("Starting crawl for {}", self.url);

        // Start the crawl
        website.crawl().await;

        // Get all visited links
        let links = website.get_all_links_visited().await;
        log::info!("Crawling completed, visited {} pages", links.len());

        Ok(())
    }
}
