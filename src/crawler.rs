use crate::config::{CrawlerConfig, CrawlingMode, ResourceType};
use spider::website::Website;
use std::error::Error;
use crate::scraper::ScraperResult;
use scraper::{Html, Selector};
use spider::url::Url;
use crate::scraper::PageScraper;
use crate::meilisearch::MeilisearchUploader;
use serde::{Serialize, Deserialize};

pub struct Crawler {
    config: CrawlerConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Resource {
    pub url: String,
    pub title: String,
    pub resource_type: ResourceType,
}

impl Crawler {
    pub fn new(config: CrawlerConfig) -> Self {
        Self { config }
    }

    // Helper function to get domain without TLD
    fn get_base_domain(url: &str) -> Result<String, Box<dyn Error>> {
        let parsed_url = Url::parse(url)
            .map_err(|e| format!("Failed to parse URL: {}", e))?;
        
        let host = parsed_url.host_str()
            .ok_or("Failed to get host from URL")?;

        // Split by dots and take the domain part (usually the second-to-last part)
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() >= 2 {
            Ok(parts[parts.len() - 2].to_string())
        } else {
            Ok(host.to_string())
        }
    }

    // Helper function to clean URLs: keep only scheme, domain, and path
    fn clean_url(url: Url) -> Url {
        let scheme = url.scheme().to_string();
        let host = url.host_str().unwrap_or("").to_string();
        let path = url.path().to_string();

        // Reconstruct URL with only scheme, host, and path
        Url::parse(&format!("{}://{}{}", scheme, host, path))
            .unwrap_or(url)
    }

    pub async fn crawl(&self) -> Result<(), Box<dyn Error>> {
        let base_domain = Self::get_base_domain(&self.config.url)?;

        let mut website = Website::new(&self.config.url)
            .with_depth(3)  // You might want to make this configurable in CrawlerConfig
            .with_respect_robots_txt(true)
            .with_subdomains(self.config.subdomains)
            .with_delay(20)
            .with_user_agent(Some("Meilisearch Scrapix Bot"))
            .build()
            .unwrap();

        // Configure based on crawling mode
        match self.config.mode {
            CrawlingMode::Http => {
                // Default HTTP configuration already set above
            }
            CrawlingMode::Chrome => {
                println!("Chrome-based crawling not yet implemented");
            }
            CrawlingMode::Smart => {
                println!("Smart crawling not yet implemented");
            }
        }

        // Apply whitelist if specified
        if let Some(whitelist) = &self.config.whitelist {
            website.with_whitelist_url(Some(whitelist.iter().map(|s| s.into()).collect()));
        }

        // Apply blacklist if specified
        if let Some(blacklist) = &self.config.blacklist {
            website.with_blacklist_url(Some(blacklist.iter().map(|s| s.into()).collect()));
        }

        let start = std::time::Instant::now();

        // Set up subscription and queue for crawling
        let mut receiver = website.subscribe(100).unwrap();
        let mut guard = website.subscribe_guard().unwrap();
        let queue = website.queue(10000).unwrap();

        let mut results = Vec::new();

        let scraper = PageScraper::new(self.config.clone());

        // Spawn a task to process pages as they're crawled
        let process_handle = tokio::spawn({
            let scraper = scraper;
            let base_domain = base_domain.clone();
            async move {
                while let Ok(page) = receiver.recv().await {
                    let result = scraper.scrape_page(&page);

                    // Queue new URLs found in the page
                    if let Ok(link_selector) = Selector::parse("a[href]") {
                        let document = Html::parse_document(&page.get_html());
                        for link in document.select(&link_selector) {
                            if let Some(href) = link.value().attr("href") {
                                if let Ok(base_url) = Url::parse(page.get_url()) {
                                    if let Ok(absolute_url) = base_url.join(href) {
                                        // Clean the URL first
                                        let cleaned_url = Self::clean_url(absolute_url);
                                        
                                        // Extract domain without TLD and compare
                                        if let Ok(host_domain) = Self::get_base_domain(cleaned_url.as_str()) {
                                            if host_domain == base_domain {
                                                let _ = queue.send(cleaned_url.into());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    results.push(result);
                    guard.inc();
                }
                results
            }
        });

        // Start the crawl
        website.crawl().await;

        // Get and display all visited links
        let links = website.get_all_links_visited().await;

        // Wait for processing to complete and collect results
        let results = process_handle.await?;

        let duration = start.elapsed();
        println!(
            "\nCrawling completed in {:?}, visited {} pages - scraped {} pages",
            duration,
            links.len(),
            results.len()
        );

        // Send results to webhook if configured
        if let Some(webhook_url) = &self.config.webhook_url {
            self.send_to_webhook(webhook_url, &results).await?;
        }

        // If Meilisearch is configured, upload the results
        if let Some(meilisearch_config) = &self.config.meilisearch {
            let uploader = MeilisearchUploader::new(meilisearch_config.clone());
            uploader.upload_documents(results).await?;
        }

        Ok(())
    }

    async fn send_to_webhook(&self, webhook_url: &str, results: &[ScraperResult]) -> Result<(), Box<dyn Error>> {
        // TODO: Implement webhook sending
        println!("Would send {} results to webhook: {}", results.len(), webhook_url);
        Ok(())
    }
} 