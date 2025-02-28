use crate::scraper::ScraperResult;
use meilisearch_sdk::client::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeilisearchConfig {
    pub url: String,
    pub api_key: String,
    pub index_name: String,

    /// Optional timeout in seconds for operations
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30 // Default 30 second timeout
}

pub struct MeilisearchUploader {
    client: Client,
    index_name: String,
    timeout: Duration,
}

impl MeilisearchUploader {
    pub fn new(config: MeilisearchConfig) -> Self {
        // Client::new returns a Result in newer versions, handle it properly
        let client = Client::new(&config.url, Some(&config.api_key))
            .expect("Failed to initialize Meilisearch client");

        let timeout = Duration::from_secs(config.timeout_secs);

        Self {
            client,
            index_name: config.index_name,
            timeout,
        }
    }

    pub async fn upload_documents(
        &self,
        documents: Vec<ScraperResult>,
    ) -> Result<(), Box<dyn Error>> {
        if documents.is_empty() {
            println!("No documents to upload to Meilisearch");
            return Ok(());
        }

        let index = self.client.index(&self.index_name);

        println!(
            "Uploading {} documents to Meilisearch index '{}'",
            documents.len(),
            self.index_name
        );

        // Add documents to the index
        let task = index
            .add_documents(&documents, Some("id"))
            .await
            .map_err(|e| format!("Failed to add documents to Meilisearch: {}", e))?;

        // Wait for the task to complete with a timeout
        match task
            .wait_for_completion(&self.client, Some(self.timeout), None)
            .await
        {
            Ok(_) => {
                println!(
                    "Successfully uploaded {} documents to Meilisearch",
                    documents.len()
                );
                Ok(())
            }
            Err(e) => {
                // Document processing might continue even if we timeout waiting
                // So we'll report this as a warning
                println!(
                    "Warning: Could not confirm upload of {} documents to Meilisearch: {}",
                    documents.len(),
                    e
                );
                // Return success anyway, as the documents were submitted
                Ok(())
            }
        }
    }
}
