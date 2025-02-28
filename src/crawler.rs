use crate::config::{CrawlerConfig, CrawlingMode, ResourceType};
use crate::meilisearch::MeilisearchUploader;
use crate::scraper::{PageScraper, ScraperResult};
use futures::future::join_all;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use spider::page::Page;
use spider::url::Url;
use spider::website::Website;
use std::collections::HashSet;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, Mutex as TokioMutex};
pub struct Crawler {
    config: CrawlerConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
        let parsed_url = Url::parse(url).map_err(|e| format!("Failed to parse URL: {}", e))?;

        let host = parsed_url.host_str().ok_or("Failed to get host from URL")?;

        // Split by dots and take the domain part (usually the second-to-last part)
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() >= 2 {
            Ok(parts[parts.len() - 2].to_string())
        } else {
            Ok(host.to_string())
        }
    }

    // Enhanced URL normalization function
    fn normalize_url(url: Url) -> Url {
        let mut normalized = url.clone();

        // Remove fragments
        normalized.set_fragment(None);

        // Remove query parameters (optional, depends on your use case)
        normalized.set_query(None);

        // Ensure path doesn't end with trailing slash for consistency
        let path = normalized.path().trim_end_matches('/');
        let non_empty_path = if path.is_empty() { "/" } else { path };

        // Fix: Clone path to avoid mutable and immutable borrow issues
        let path_to_set = non_empty_path.to_string();
        let _ = normalized.set_path(&path_to_set);

        normalized
    }

    // Helper function to check if a URL should be processed
    fn should_process_url(url: &Url, base_domain: &str) -> bool {
        if let Ok(host_domain) = Self::get_base_domain(url.as_str()) {
            return host_domain == base_domain;
        }
        false
    }

    // Extract and process links from a page
    fn extract_links(page: &Page, base_domain: &str) -> Vec<Url> {
        let mut extracted_urls = Vec::new();

        if let Ok(link_selector) = Selector::parse("a[href]") {
            let document = Html::parse_document(&page.get_html());

            for link in document.select(&link_selector) {
                if let Some(href) = link.value().attr("href") {
                    if let Ok(base_url) = Url::parse(page.get_url()) {
                        if let Ok(absolute_url) = base_url.join(href) {
                            // Normalize URL for better deduplication
                            let normalized_url = Self::normalize_url(absolute_url);

                            // Only collect URLs within the same domain
                            if Self::should_process_url(&normalized_url, base_domain) {
                                extracted_urls.push(normalized_url);
                            }
                        }
                    }
                }
            }
        }

        extracted_urls
    }

    // Process extracted URLs with deduplication
    fn process_urls(
        urls: Vec<Url>,
        processed_urls: &Arc<Mutex<HashSet<String>>>,
        url_sender: &mpsc::Sender<String>,
    ) -> usize {
        let mut new_urls = 0;

        for url in urls {
            let url_string = url.to_string();

            // Check if URL has been seen before
            let should_queue = {
                let url_set = processed_urls.lock().unwrap();
                !url_set.contains(&url_string)
            };

            if should_queue {
                // Only queue URLs that haven't been seen
                match url_sender.try_send(url_string.clone()) {
                    Ok(_) => {
                        // Mark as queued only after successfully sending
                        let mut url_set = processed_urls.lock().unwrap();
                        url_set.insert(url_string);
                        new_urls += 1;
                    }
                    Err(e) => {
                        log::warn!("Failed to queue URL: {} - Error: {}", url_string, e);
                    }
                }
            }
        }

        new_urls
    }

    // Process a single page (extract links and scrape content)
    async fn process_page(
        page: &Page,
        worker_id: usize,
        scraper: &PageScraper,
        base_domain: &str,
        processed_urls: &Arc<Mutex<HashSet<String>>>,
        url_sender: &mpsc::Sender<String>,
        results_tx: &mpsc::Sender<ScraperResult>,
    ) -> Result<(), Box<dyn Error>> {
        let url_string = page.get_url().to_string();

        // Check if we should log URL processing (only applicable for worker_id % 4 == 0)
        if worker_id % 4 == 0 {
            log::debug!("Worker {} processing URL: {}", worker_id, url_string);
        }

        // Extract links
        let extracted_urls = Self::extract_links(page, base_domain);

        // Process and deduplicate URLs
        let new_urls = Self::process_urls(extracted_urls, processed_urls, url_sender);

        if new_urls > 0 && worker_id % 4 == 0 {
            // Only log from some workers to reduce output
            log::info!("Worker {} found {} new URLs", worker_id, new_urls);
        }

        // Scrape the page content
        let result = scraper.scrape_page(page);

        // Send the result through the channel
        if let Err(e) = results_tx.send(result).await {
            log::error!("Worker {} failed to send result: {}", worker_id, e);
        }

        log::debug!("Sending result for URL: {}", page.get_url());

        Ok(())
    }

    // Initialize a Website object with all configurations
    fn setup_website(&self) -> Result<Website, Box<dyn Error>> {
        // Set default values if not specified in config
        let crawl_depth = self.config.crawl_depth.unwrap_or(3) as usize;
        let crawl_delay = self.config.crawl_delay.unwrap_or(10);

        log::info!(
            "Setting up crawler with depth: {}, delay: {}ms",
            crawl_depth,
            crawl_delay
        );

        let mut website = Website::new(&self.config.url)
            .with_depth(crawl_depth)
            .with_respect_robots_txt(self.config.respect_robots_txt.unwrap_or(true))
            .with_subdomains(self.config.subdomains)
            .with_delay(crawl_delay)
            .with_user_agent(Some("Meilisearch Scrapix Bot"))
            .build()
            .map_err(|e| format!("Failed to build website crawler: {}", e))?;

        // Configure based on crawling mode
        match self.config.mode {
            CrawlingMode::Http => {
                // Default HTTP configuration already set above
            }
            CrawlingMode::Chrome => {
                log::info!("Chrome-based crawling not yet implemented");
            }
            CrawlingMode::Smart => {
                log::info!("Smart crawling not yet implemented");
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

        Ok(website)
    }

    // Setup a worker for processing pages
    fn setup_worker(
        &self,
        worker_id: usize,
        website: &mut Website,
        base_domain: &str,
        processed_urls: Arc<Mutex<HashSet<String>>>,
        url_sender: mpsc::Sender<String>,
        results_tx: mpsc::Sender<ScraperResult>,
    ) -> Result<tokio::task::JoinHandle<Result<usize, Box<dyn Error + Send + Sync>>>, Box<dyn Error>>
    {
        // Each worker needs its own subscription
        let mut receiver = match website.subscribe(100) {
            Some(r) => r,
            None => {
                return Err(
                    format!("Failed to create subscription for worker {}", worker_id).into(),
                )
            }
        };

        let mut worker_guard = match website.subscribe_guard() {
            Some(g) => g,
            None => {
                return Err(format!(
                    "Failed to create subscription guard for worker {}",
                    worker_id
                )
                .into())
            }
        };

        let worker_scraper = PageScraper::new(self.config.clone());

        // Create a String from the &str for the async closure
        let base_domain = base_domain.to_string();

        let handle = tokio::spawn(async move {
            let mut pages_processed = 0;

            log::info!("Worker {} started", worker_id);

            while let Ok(page) = receiver.recv().await {
                // Process the page (extract links and scrape content)
                let _ = Self::process_page(
                    &page,
                    worker_id,
                    &worker_scraper,
                    &base_domain,
                    &processed_urls,
                    &url_sender,
                    &results_tx,
                )
                .await;

                // Increment pages processed counter
                pages_processed += 1;

                // Increment the subscription guard
                worker_guard.inc();

                // Periodically log progress
                if pages_processed % 100 == 0 {
                    log::info!(
                        "Worker {} has processed {} pages",
                        worker_id,
                        pages_processed
                    );
                }
            }

            log::info!(
                "Worker {} finished, processed {} pages",
                worker_id,
                pages_processed
            );
            Ok(pages_processed)
        });

        Ok(handle)
    }

    // URL queue manager that handles adding new URLs to the website
    async fn url_queue_manager(
        website: Arc<TokioMutex<Website>>,
        mut url_receiver: mpsc::Receiver<String>,
    ) {
        log::info!("URL queue manager started");

        let mut urls_queued = 0;
        let mut retry_urls = Vec::new();
        let mut last_queue_check = std::time::Instant::now();
        let queue_check_interval = std::time::Duration::from_secs(30);

        while let Some(url_string) = url_receiver.recv().await {
            if let Ok(url) = Url::parse(&url_string) {
                let mut website = website.lock().await;
                if let Some(queue) = website.queue(10000) {
                    if let Err(e) = queue.send(url.into()) {
                        log::warn!("Failed to queue URL: {} - Error: {}", url_string, e);
                        // Save for retry
                        retry_urls.push(url_string);
                    } else {
                        urls_queued += 1;

                        if urls_queued % 100 == 0 {
                            log::info!("Queue manager: {} URLs queued so far", urls_queued);
                        }
                    }
                } else {
                    log::warn!("Failed to get queue for URL: {}", url_string);
                    retry_urls.push(url_string);
                }
            }

            // Periodically attempt to retry failed URLs
            let now = std::time::Instant::now();
            if now.duration_since(last_queue_check) > queue_check_interval && !retry_urls.is_empty()
            {
                log::info!("Attempting to retry {} failed URLs", retry_urls.len());
                let mut success_count = 0;

                // Create a copy of retry_urls for processing
                let retry_batch = retry_urls.clone();
                retry_urls.clear();

                for retry_url in &retry_batch {
                    if let Ok(url) = Url::parse(retry_url) {
                        let mut website = website.lock().await;
                        if let Some(queue) = website.queue(10000) {
                            if queue.send(url.into()).is_ok() {
                                urls_queued += 1;
                                success_count += 1;
                            } else {
                                retry_urls.push(retry_url.clone());
                            }
                        } else {
                            retry_urls.push(retry_url.clone());
                        }
                    }
                }

                log::info!(
                    "Retry attempt: re-queued {}/{} URLs",
                    success_count,
                    retry_batch.len()
                );
                last_queue_check = now;
            }
        }

        // Final attempt to process any remaining retry URLs
        if !retry_urls.is_empty() {
            log::info!("Final attempt to queue {} remaining URLs", retry_urls.len());
            let mut success_count = 0;

            // Create a copy for the final attempt
            let final_retry_urls = retry_urls.clone();

            for retry_url in &final_retry_urls {
                if let Ok(url) = Url::parse(retry_url) {
                    let mut website = website.lock().await;
                    if let Some(queue) = website.queue(10000) {
                        if queue.send(url.into()).is_ok() {
                            urls_queued += 1;
                            success_count += 1;
                        }
                    }
                }
            }

            log::info!(
                "Final retry: queued {}/{} URLs",
                success_count,
                final_retry_urls.len()
            );
        }

        log::info!(
            "URL queue manager finished, queued {} URLs total",
            urls_queued
        );
    }

    // Process a chunk of results and add them to a collected vector
    async fn process_results_chunk(
        results: &[ScraperResult],
        chunk_idx: usize,
        all_collected_results: &mut Vec<ScraperResult>,
    ) {
        // Clone the results and add them to the collected results vector
        all_collected_results.extend(results.to_vec());

        // This is where you could implement other processing like saving to a file or database
        // For example: write_to_file(results, format!("chunk_{}.json", chunk_idx)).await;

        log::info!(
            "Processed chunk {} with {} results",
            chunk_idx,
            results.len()
        );
    }

    // Upload results to Meilisearch in batches
    async fn upload_to_meilisearch(
        &self,
        results: Vec<ScraperResult>,
        meilisearch_config: &crate::meilisearch::MeilisearchConfig,
    ) -> Result<(), Box<dyn Error>> {
        let uploader = MeilisearchUploader::new(meilisearch_config.clone());

        // Determine batch size - default to 100 if not specified
        let batch_size = self.config.meilisearch_batch_size.unwrap_or(100);

        log::info!("Uploading to Meilisearch in batches of {}", batch_size);

        // Upload in batches
        for (i, chunk) in results.chunks(batch_size).enumerate() {
            uploader.upload_documents(chunk.to_vec()).await?;
            log::info!("Uploaded batch {} ({} documents)", i + 1, chunk.len());
        }

        Ok(())
    }

    pub async fn crawl(&self) -> Result<(), Box<dyn Error>> {
        let base_domain = Self::get_base_domain(&self.config.url)?;
        let worker_count = self.config.concurrency.unwrap_or(16);

        log::info!("Starting crawl with {} workers", worker_count);

        // Setup website with all configurations
        let mut website = self.setup_website()?;

        // Create a shared set to track processed URLs to avoid duplicates
        let processed_urls = Arc::new(Mutex::new(HashSet::new()));

        // Ensure seed URL is in the processed set to avoid re-processing
        let seed_url = self.config.url.clone();
        processed_urls.lock().unwrap().insert(seed_url);

        // Create a channel for new URLs to be queued
        let (url_sender, url_receiver) = mpsc::channel::<String>(10000);

        // Wrap the website in a TokioMutex for shared access
        let website_mutex = Arc::new(TokioMutex::new(website.clone()));

        // Start the URL queue manager
        let url_manager_handle =
            tokio::spawn(Self::url_queue_manager(website_mutex.clone(), url_receiver));

        let start = std::time::Instant::now();

        // Create a channel for collecting results from workers
        let (results_tx, mut results_rx) = mpsc::channel::<ScraperResult>(10000);

        // Create worker pool
        let mut worker_handles = Vec::with_capacity(worker_count);

        // Start workers
        for worker_id in 0..worker_count {
            let worker_handle = self.setup_worker(
                worker_id,
                &mut website,
                &base_domain,
                Arc::clone(&processed_urls),
                url_sender.clone(),
                results_tx.clone(),
            )?;

            worker_handles.push(worker_handle);
        }

        // Drop the original senders to ensure channels close when all workers are done
        drop(results_tx);
        drop(url_sender);

        // Start a separate task to collect results
        let results_handle = tokio::spawn(async move {
            let mut current_chunk = Vec::new();
            let mut all_results = Vec::new();
            let mut chunk_count = 0;
            let chunk_size = 50;

            // Define a helper closure for processing chunks
            let process_chunk =
                |chunk: &[ScraperResult], idx: usize, all: &mut Vec<ScraperResult>| {
                    // Clone the results and add them to the collected results vector
                    all.extend(chunk.to_vec());
                    log::info!("Processed chunk {} with {} results", idx, chunk.len());
                };

            while let Some(result) = results_rx.recv().await {
                current_chunk.push(result);

                // Log progress periodically based on the total count
                if (all_results.len() + current_chunk.len()) % 500 == 0 && !current_chunk.is_empty()
                {
                    log::info!(
                        "Collected {} results so far",
                        all_results.len() + current_chunk.len()
                    );
                }

                // Process in chunks to avoid memory issues
                if current_chunk.len() >= chunk_size {
                    // Process the current chunk
                    process_chunk(&current_chunk, chunk_count, &mut all_results);

                    chunk_count += 1;

                    // Clear the current chunk to free memory
                    current_chunk.clear();
                }
            }

            // Process any remaining results
            if !current_chunk.is_empty() {
                process_chunk(&current_chunk, chunk_count, &mut all_results);
            }

            log::info!(
                "Total processed: {} results in {} chunks",
                all_results.len(),
                chunk_count + 1
            );
            all_results
        });

        // Start the crawl - this runs in parallel with the workers
        log::info!("Starting crawl process...");

        // Create a timeout for the website crawl
        let crawl_future = website.crawl();
        match tokio::time::timeout(std::time::Duration::from_secs(3600), crawl_future).await {
            Ok(_) => log::info!("Website crawl completed within time limit"),
            Err(_) => {
                log::warn!("Website crawl timed out after 1 hour - continuing with processing")
            }
        }

        log::info!("Crawl process completed, waiting for workers to finish processing...");

        // Check if workers are still active before joining them
        log::info!("Checking if workers are still active...");
        for (i, handle) in worker_handles.iter().enumerate() {
            if handle.is_finished() {
                log::warn!("Worker {} finished prematurely!", i);
            }
        }

        // Track worker activity with periodic status checks
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        let max_checks = 10;
        let mut checks_done = 0;

        while checks_done < max_checks {
            interval.tick().await;
            checks_done += 1;

            let active_count = worker_handles.iter().filter(|h| !h.is_finished()).count();
            log::info!(
                "{}/{} workers still active (check {}/{})",
                active_count,
                worker_handles.len(),
                checks_done,
                max_checks
            );

            if active_count == 0 {
                log::warn!(
                    "All workers have finished, but results collection may still be ongoing"
                );
                break;
            }
        }

        // Wait for all workers to complete
        let worker_results = join_all(worker_handles).await;

        let total_pages_processed: usize = worker_results
            .into_iter()
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .sum();

        // Wait for URL manager to complete
        let _ = url_manager_handle.await;

        // Get all results
        let results = results_handle
            .await
            .map_err(|e| format!("Failed to join results: {}", e))?;

        // Get all visited links
        let links = website.get_all_links_visited().await;

        let unique_urls_count = processed_urls.lock().unwrap().len();

        let duration = start.elapsed();
        log::info!("\nCrawling completed in {:?}", duration);
        log::info!(
            "Visited {} pages - scraped {} pages - discovered {} unique URLs",
            links.len(),
            total_pages_processed,
            unique_urls_count
        );

        // Process results based on configured outputs
        if let Some(webhook_url) = &self.config.webhook_url {
            self.send_to_webhook(webhook_url, &results).await?;
        }

        // If Meilisearch is configured, upload the results in batches
        if let Some(meilisearch_config) = &self.config.meilisearch {
            self.upload_to_meilisearch(results, meilisearch_config)
                .await?;
        }

        Ok(())
    }

    async fn send_to_webhook(
        &self,
        webhook_url: &str,
        results: &[ScraperResult],
    ) -> Result<(), Box<dyn Error>> {
        // TODO: Implement webhook sending with proper batching
        log::info!(
            "Sending {} results to webhook: {}",
            results.len(),
            webhook_url
        );

        // For larger result sets, consider batching webhook calls as well
        let batch_size = 100;
        for (i, chunk) in results.chunks(batch_size).enumerate() {
            log::info!(
                "Would send batch {} ({} documents) to webhook",
                i + 1,
                chunk.len()
            );
            // Actual webhook implementation would go here
        }

        Ok(())
    }
}
