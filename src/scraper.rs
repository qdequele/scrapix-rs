use crate::config::CrawlerConfig;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use spider::page::Page;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Resource {
    pub url: String,
    pub title: String,
    pub resource_type: crate::config::ResourceType,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScraperResult {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub h1: Option<Vec<String>>,
    pub h2: Option<Vec<String>>,
    pub h3: Option<Vec<String>>,
    pub h4: Option<Vec<String>>,
    pub h5: Option<Vec<String>>,
    pub h6: Option<Vec<String>>,
    pub p: Option<Vec<String>>,
    pub content: Option<String>,
    pub markdown: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    pub custom_fields: Option<HashMap<String, String>>,
    pub schema_org: Option<serde_json::Value>,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone)]
pub struct Scraper {
    config: CrawlerConfig,
}

impl Scraper {
    pub fn new(config: CrawlerConfig) -> Self {
        Self { config }
    }

    // Helper function to extract text elements by selector
    fn extract_elements(&self, document: &Html, selector_str: &str) -> Option<Vec<String>> {
        if let Ok(selector) = Selector::parse(selector_str) {
            let texts: Vec<String> = document
                .select(&selector)
                .map(|el| el.text().collect::<String>())
                .collect();

            if !texts.is_empty() {
                return Some(texts);
            }
        }
        None
    }

    // Helper function to extract a single text element
    fn extract_single_element(&self, document: &Html, selector_str: &str) -> Option<String> {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                return Some(element.text().collect::<String>());
            }
        }
        None
    }

    // Helper function to extract meta tag content
    fn extract_meta_tags(
        &self,
        document: &Html,
        attribute_selector: &str,
        name_attr: &str,
        content_attr: &str,
    ) -> HashMap<String, String> {
        let mut meta_data = HashMap::new();

        if let Ok(selector) = Selector::parse(attribute_selector) {
            for element in document.select(&selector) {
                if let (Some(name), Some(content)) = (
                    element.value().attr(name_attr),
                    element.value().attr(content_attr),
                ) {
                    meta_data.insert(name.to_string(), content.to_string());
                }
            }
        }

        meta_data
    }

    // Helper function to extract resources by selector
    fn extract_resource_type(
        &self,
        document: &Html,
        selector_str: &str,
        url_attr: &str,
        title_source: &str,
        resource_type: crate::config::ResourceType,
    ) -> Vec<Resource> {
        let mut resources = Vec::new();

        if let Ok(selector) = Selector::parse(selector_str) {
            for element in document.select(&selector) {
                if let Some(url) = element.value().attr(url_attr) {
                    let title = match title_source {
                        "text" => element.text().collect::<String>(),
                        attr => element.value().attr(attr).unwrap_or_default().to_string(),
                    };

                    resources.push(Resource {
                        url: url.to_string(),
                        title,
                        resource_type: resource_type.clone(),
                    });
                }
            }
        }

        resources
    }

    pub fn scrape_page(&self, page: &Page) -> ScraperResult {
        // Use info level for scraping logs so the user can see activity
        log::info!("Scraping page: {}", page.get_url());

        let mut result = ScraperResult {
            id: uuid::Uuid::new_v4().to_string(),
            url: page.get_url().to_string(),
            title: None,
            h1: None,
            h2: None,
            h3: None,
            h4: None,
            h5: None,
            h6: None,
            p: None,
            content: None,
            markdown: None,
            metadata: None,
            custom_fields: None,
            schema_org: None,
            resources: Vec::new(),
        };

        let html = page.get_html();
        let document = Html::parse_document(&html);

        self.extract_title(&document, &mut result);
        self.extract_content(&document, &mut result);
        self.extract_markdown(&html, &mut result);
        self.extract_metadata(&document, &mut result);
        self.extract_custom_fields(&document, &mut result);
        self.extract_schema_org(&document, &mut result);
        self.extract_resources(&document, &mut result);

        result
    }

    fn extract_title(&self, document: &Html, result: &mut ScraperResult) {
        result.title = self.extract_single_element(document, "title");
    }

    fn extract_content(&self, document: &Html, result: &mut ScraperResult) {
        if !self.config.split_content {
            // Extract headings and paragraphs using helper method
            result.h1 = self.extract_elements(document, "h1");
            result.h2 = self.extract_elements(document, "h2");
            result.h3 = self.extract_elements(document, "h3");
            result.h4 = self.extract_elements(document, "h4");
            result.h5 = self.extract_elements(document, "h5");
            result.h6 = self.extract_elements(document, "h6");
            result.p = self.extract_elements(document, "p, span, td, th, li");
        }
    }

    fn extract_markdown(&self, html: &str, result: &mut ScraperResult) {
        if self.config.extract_markdown {
            result.markdown = Some(html2md::parse_html(html));
        }
    }

    fn extract_metadata(&self, document: &Html, result: &mut ScraperResult) {
        if self.config.extract_metadata {
            let mut metadata = HashMap::new();

            // Extract title with helper method
            if let Some(title) = self.extract_single_element(document, "title") {
                metadata.insert("title".to_string(), title);
            }

            // Extract standard meta tags
            let standard_meta =
                self.extract_meta_tags(document, "meta[name][content]", "name", "content");
            metadata.extend(standard_meta);

            // Extract OpenGraph meta tags
            let og_meta = self.extract_meta_tags(
                document,
                "meta[property^='og:'][content]",
                "property",
                "content",
            );
            metadata.extend(og_meta);

            if !metadata.is_empty() {
                result.metadata = Some(metadata);
            }
        }
    }

    fn extract_custom_fields(&self, document: &Html, result: &mut ScraperResult) {
        if let Some(custom_fields) = &self.config.extract_custom_fields {
            let mut field_results = HashMap::new();

            for (key, selector_str) in custom_fields {
                if let Ok(selector) = Selector::parse(selector_str) {
                    let value: String = document
                        .select(&selector)
                        .map(|element| element.text().collect::<String>())
                        .collect::<Vec<String>>()
                        .join(" ")
                        .trim()
                        .to_string();

                    if !value.is_empty() {
                        field_results.insert(key.clone(), value);
                    }
                }
            }

            if !field_results.is_empty() {
                result.custom_fields = Some(field_results);
            }
        }
    }

    fn extract_schema_org(&self, document: &Html, result: &mut ScraperResult) {
        if self.config.extract_schema_org {
            if let Ok(script_selector) = Selector::parse("script[type='application/ld+json']") {
                let schema_data: Vec<serde_json::Value> = document
                    .select(&script_selector)
                    .filter_map(|element| element.text().collect::<String>().parse().ok())
                    .collect();

                if !schema_data.is_empty() {
                    result.schema_org = Some(serde_json::Value::Array(schema_data));
                }
            }
        }
    }

    fn extract_resources(&self, document: &Html, result: &mut ScraperResult) {
        // Extract resources by type using the helper function
        let mut resources = Vec::new();

        // Extract images
        let images = self.extract_resource_type(
            document,
            "img[src]",
            "src",
            "alt",
            crate::config::ResourceType::Image,
        );
        resources.extend(images);

        // Extract PDFs
        let pdfs = self.extract_resource_type(
            document,
            "a[href$='.pdf']",
            "href",
            "text",
            crate::config::ResourceType::Pdf,
        );
        resources.extend(pdfs);

        // Extract JSON files
        let jsons = self.extract_resource_type(
            document,
            "a[href$='.json']",
            "href",
            "text",
            crate::config::ResourceType::Json,
        );
        resources.extend(jsons);

        result.resources = resources;
    }
}
