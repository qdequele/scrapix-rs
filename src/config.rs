use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::meilisearch::MeilisearchConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CrawlingMode {
    Http,
    Chrome,
    Smart,
}

impl Default for CrawlingMode {
    fn default() -> Self {
        Self::Http
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ResourceType {
    Pdf,
    Image,
    Json,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrawlerConfig {
    pub url: String,

    #[serde(default)]
    pub mode: CrawlingMode,

    #[serde(default)]
    pub subdomains: bool,
    
    #[serde(default)]
    pub resources: Option<Vec<ResourceType>>,
    
    #[serde(default)]
    pub split_content: bool,
    
    #[serde(default)]
    pub extract_markdown: bool,
    
    #[serde(default)]
    pub extract_metadata: bool,
    
    #[serde(default)]
    pub extract_custom_fields: Option<HashMap<String, String>>,
    
    #[serde(default)]
    pub extract_schema_org: bool,
    
    #[serde(default)]
    pub webhook_url: Option<String>,
    
    #[serde(default)]
    pub whitelist: Option<Vec<String>>,
    
    #[serde(default)]
    pub blacklist: Option<Vec<String>>,

    #[serde(default)]
    pub meilisearch: Option<MeilisearchConfig>,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            mode: CrawlingMode::Http,
            subdomains: false,
            resources: None,
            split_content: false,
            extract_markdown: false,
            extract_metadata: false,
            extract_custom_fields: None,
            extract_schema_org: false,
            webhook_url: None,
            whitelist: None,
            blacklist: None,
            meilisearch: None,
        }
    }
} 