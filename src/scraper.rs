use crate::config::CrawlerConfig;
use crate::crawler::Resource;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use spider::page::Page;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
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

pub struct PageScraper {
    config: CrawlerConfig,
}

impl PageScraper {
    pub fn new(config: CrawlerConfig) -> Self {
        Self { config }
    }

    pub fn scrape_page(&self, page: &Page) -> ScraperResult {
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
        if let Ok(title_selector) = Selector::parse("title") {
            if let Some(title_element) = document.select(&title_selector).next() {
                result.title = Some(title_element.text().collect::<String>());
            }
        }
    }

    fn extract_content(&self, document: &Html, result: &mut ScraperResult) {
        if !self.config.split_content {
            // Extract headings and paragraphs
            if let Ok(h1_selector) = Selector::parse("h1") {
                let h1_texts: Vec<String> = document
                    .select(&h1_selector)
                    .map(|el| el.text().collect::<String>())
                    .collect();
                if !h1_texts.is_empty() {
                    result.h1 = Some(h1_texts);
                }
            }

            if let Ok(h2_selector) = Selector::parse("h2") {
                let h2_texts: Vec<String> = document
                    .select(&h2_selector)
                    .map(|el| el.text().collect::<String>())
                    .collect();
                if !h2_texts.is_empty() {
                    result.h2 = Some(h2_texts);
                }
            }

            if let Ok(h3_selector) = Selector::parse("h3") {
                let h3_texts: Vec<String> = document
                    .select(&h3_selector)
                    .map(|el| el.text().collect::<String>())
                    .collect();
                if !h3_texts.is_empty() {
                    result.h3 = Some(h3_texts);
                }
            }

            if let Ok(h4_selector) = Selector::parse("h4") {
                let h4_texts: Vec<String> = document
                    .select(&h4_selector)
                    .map(|el| el.text().collect::<String>())
                    .collect();
                if !h4_texts.is_empty() {
                    result.h4 = Some(h4_texts);
                }
            }

            if let Ok(h5_selector) = Selector::parse("h5") {
                let h5_texts: Vec<String> = document
                    .select(&h5_selector)
                    .map(|el| el.text().collect::<String>())
                    .collect();
                if !h5_texts.is_empty() {
                    result.h5 = Some(h5_texts);
                }
            }

            if let Ok(h6_selector) = Selector::parse("h6") {
                let h6_texts: Vec<String> = document
                    .select(&h6_selector)
                    .map(|el| el.text().collect::<String>())
                    .collect();
                if !h6_texts.is_empty() {
                    result.h6 = Some(h6_texts);
                }
            }

            if let Ok(p_selector) = Selector::parse("p, span, td, th, li") {
                let p_texts: Vec<String> = document
                    .select(&p_selector)
                    .map(|el| el.text().collect::<String>())
                    .collect();
                if !p_texts.is_empty() {
                    result.p = Some(p_texts);
                }
            }
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

            // Extract title
            if let Ok(title_selector) = Selector::parse("title") {
                if let Some(title_element) = document.select(&title_selector).next() {
                    metadata.insert(
                        "title".to_string(),
                        title_element.text().collect::<String>(),
                    );
                }
            }

            // Extract meta tags
            if let Ok(meta_selector) = Selector::parse("meta[name][content]") {
                for element in document.select(&meta_selector) {
                    if let Some(name) = element.value().attr("name") {
                        if let Some(content) = element.value().attr("content") {
                            metadata.insert(name.to_string(), content.to_string());
                        }
                    }
                }
            }

            // Extract OpenGraph meta tags
            if let Ok(og_selector) = Selector::parse("meta[property^='og:'][content]") {
                for element in document.select(&og_selector) {
                    if let Some(property) = element.value().attr("property") {
                        if let Some(content) = element.value().attr("content") {
                            metadata.insert(property.to_string(), content.to_string());
                        }
                    }
                }
            }

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
        // Extract images
        if let Ok(img_selector) = Selector::parse("img[src]") {
            for img in document.select(&img_selector) {
                if let Some(src) = img.value().attr("src") {
                    let title = img.value().attr("alt").unwrap_or_default().to_string();
                    result.resources.push(Resource {
                        url: src.to_string(),
                        title: title,
                        resource_type: crate::config::ResourceType::Image,
                    });
                }
            }
        }

        // Expract PDF
        if let Ok(pdf_selector) = Selector::parse("a[href$='.pdf']") {
            for pdf in document.select(&pdf_selector) {
                if let Some(href) = pdf.value().attr("href") {
                    let title = pdf.text().collect::<String>().to_string();
                    result.resources.push(Resource {
                        url: href.to_string(),
                        title: title,
                        resource_type: crate::config::ResourceType::Pdf,
                    });
                }
            }
        }

        // Extract JSON files
        if let Ok(json_selector) = Selector::parse("a[href$='.json']") {
            for json in document.select(&json_selector) {
                if let Some(href) = json.value().attr("href") {
                    let title = json.text().collect::<String>().to_string();
                    result.resources.push(Resource {
                        url: href.to_string(),
                        title: title,
                        resource_type: crate::config::ResourceType::Json,
                    });
                }
            }
        }
    }
}
