use anyhow::{Context, Result};
use serde::Deserialize;

pub struct ConfluenceClient {
    base_url: String,
    auth_header: String,
    agent: ureq::Agent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfluencePage {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub body: Option<PageBody>,
    #[serde(default)]
    pub version: Option<PageVersion>,
    #[serde(rename = "_links", default)]
    pub links: Option<PageLinks>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageBody {
    #[serde(default)]
    pub view: Option<BodyContent>,
    #[serde(default)]
    pub storage: Option<BodyContent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BodyContent {
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageVersion {
    pub number: i64,
    #[serde(rename = "when", default)]
    pub when_str: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageLinks {
    #[serde(rename = "webui", default)]
    pub webui: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<ConfluencePage>,
    #[serde(default)]
    _links: Option<SearchLinks>,
}

#[derive(Debug, Deserialize)]
struct SearchLinks {
    #[serde(default)]
    next: Option<String>,
}

impl ConfluenceClient {
    pub fn new(base_url: &str, pat: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            base_url,
            auth_header: format!("Bearer {pat}"),
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
        }
    }

    pub fn new_basic(base_url: &str, email: &str, api_token: &str) -> Self {
        use std::io::Write;
        let base_url = base_url.trim_end_matches('/').to_string();
        let mut buf = Vec::new();
        write!(buf, "{email}:{api_token}").unwrap();
        let encoded = base64_encode(&buf);
        Self {
            base_url,
            auth_header: format!("Basic {encoded}"),
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
        }
    }

    pub fn get_page(&self, page_id: &str) -> Result<ConfluencePage> {
        let url = format!(
            "{}/wiki/rest/api/content/{}?expand=body.view,body.storage,version",
            self.base_url, page_id
        );
        let resp: ConfluencePage = self.agent
            .get(&url)
            .set("Authorization", &self.auth_header)
            .call()
            .context("Confluence API request failed")?
            .into_json()
            .context("Failed to parse Confluence page response")?;
        Ok(resp)
    }

    pub fn get_pages_in_space(&self, space_key: &str, limit: usize) -> Result<Vec<ConfluencePage>> {
        let mut all_pages = Vec::new();
        let mut start = 0;
        loop {
            let url = format!(
                "{}/wiki/rest/api/content?spaceKey={}&type=page&expand=body.view,version&limit={}&start={}",
                self.base_url, space_key, limit.min(50), start
            );
            let resp: SearchResponse = self.agent
                .get(&url)
                .set("Authorization", &self.auth_header)
                .call()
                .with_context(|| format!("fetch pages in space {space_key}"))?
                .into_json()
                .context("parse space pages response")?;

            let count = resp.results.len();
            all_pages.extend(resp.results);

            if count == 0 || all_pages.len() >= limit || resp._links.and_then(|l| l.next).is_none() {
                break;
            }
            start += count;
        }
        Ok(all_pages)
    }

    pub fn search_cql(&self, cql: &str, limit: usize) -> Result<Vec<ConfluencePage>> {
        let mut all_pages = Vec::new();
        let mut start = 0;
        loop {
            let url = format!(
                "{}/wiki/rest/api/content/search?cql={}&expand=body.view,version&limit={}&start={}",
                self.base_url,
                urlencoding_simple(cql),
                limit.min(50),
                start
            );
            let resp: SearchResponse = self.agent
                .get(&url)
                .set("Authorization", &self.auth_header)
                .call()
                .with_context(|| format!("CQL search: {cql}"))?
                .into_json()
                .context("parse CQL search response")?;

            let count = resp.results.len();
            all_pages.extend(resp.results);

            if count == 0 || all_pages.len() >= limit || resp._links.and_then(|l| l.next).is_none() {
                break;
            }
            start += count;
        }
        Ok(all_pages)
    }

    pub fn get_pages_modified_since(&self, space_key: &str, since: &str, limit: usize) -> Result<Vec<ConfluencePage>> {
        let cql = format!("space = \"{}\" AND type = page AND lastModified >= \"{}\"", space_key, since);
        self.search_cql(&cql, limit)
    }

    pub fn get_all_page_ids_in_space(&self, space_key: &str) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut start = 0;
        loop {
            let url = format!(
                "{}/wiki/rest/api/content?spaceKey={}&type=page&limit=200&start={}",
                self.base_url, space_key, start
            );
            let resp: SearchResponse = self.agent
                .get(&url)
                .set("Authorization", &self.auth_header)
                .call()
                .with_context(|| format!("list page IDs in space {space_key}"))?
                .into_json()
                .context("parse page ID listing")?;

            let count = resp.results.len();
            ids.extend(resp.results.into_iter().map(|p| p.id));

            if count == 0 || resp._links.and_then(|l| l.next).is_none() {
                break;
            }
            start += count;
        }
        Ok(ids)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}
