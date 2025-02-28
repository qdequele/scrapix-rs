use crate::scraper::ScraperResult;
use meilisearch_sdk::client::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeilisearchConfig {
    pub url: String,
    pub api_key: String,
    pub index_name: String,
}

pub struct MeilisearchUploader {
    client: Client,
    index_name: String,
}

impl MeilisearchUploader {
    pub fn new(config: MeilisearchConfig) -> Self {
        let client = Client::new(config.url, Some(config.api_key)).unwrap();
        Self {
            client,
            index_name: config.index_name,
        }
    }

    pub async fn upload_documents(
        &self,
        documents: Vec<ScraperResult>,
    ) -> Result<(), Box<dyn Error>> {
        let index = self.client.index(&self.index_name);

        // Add documents to the index
        let task = index.add_documents(&documents, Some("id")).await?;

        // Wait for the task to complete
        task.wait_for_completion(&self.client, None, None).await?;

        println!(
            "Successfully uploaded {} documents to Meilisearch",
            documents.len()
        );
        Ok(())
    }
}
