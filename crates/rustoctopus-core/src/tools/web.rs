use async_trait::async_trait;
use regex::Regex;
use serde_json::json;

use super::traits::{Tool, ToolError};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn validate_url(url: &str) -> Result<(), ToolError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ToolError::InvalidParams(
            "URL must start with http:// or https://".to_string(),
        ));
    }

    // Extract the host portion: after "://" and before the next "/" or end
    let after_scheme = if let Some(stripped) = url.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = url.strip_prefix("http://") {
        stripped
    } else {
        url
    };

    let host = after_scheme.split('/').next().unwrap_or("");
    if host.is_empty() {
        return Err(ToolError::InvalidParams(
            "URL must have a host".to_string(),
        ));
    }

    Ok(())
}

fn strip_tags(html: &str) -> String {
    // First remove <script>...</script> and <style>...</style> blocks (case-insensitive)
    let script_re = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let style_re = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();

    let result = script_re.replace_all(html, "");
    let result = style_re.replace_all(&result, "");

    // Strip all remaining HTML tags
    let tag_re = Regex::new(r"<[^>]*>").unwrap();
    let result = tag_re.replace_all(&result, "");

    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn collapse_whitespace(s: &str) -> String {
    let ws_re = Regex::new(r"\s+").unwrap();
    ws_re.replace_all(s.trim(), " ").to_string()
}

// ---------------------------------------------------------------------------
// WebSearchTool
// ---------------------------------------------------------------------------

pub struct WebSearchTool {
    api_key: Option<String>,
}

impl WebSearchTool {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web. Returns titles, URLs, and snippets."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "count": {
                    "type": "integer",
                    "description": "Results (1-10)",
                    "minimum": 1,
                    "maximum": 10
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let api_key = self.api_key.as_deref().ok_or_else(|| {
            ToolError::ExecutionFailed(
                "Web search not configured: no Brave API key".to_string(),
            )
        })?;

        let query = params["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: query".into()))?;

        let count = params["count"].as_u64().unwrap_or(5);

        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .query(&[("q", query), ("count", &count.to_string())])
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Search request failed: {e}")))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse response: {e}")))?;

        let results = body["web"]["results"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        if results.is_empty() {
            return Ok("No results found".to_string());
        }

        let mut output = String::new();
        for (i, result) in results.iter().enumerate() {
            let title = result["title"].as_str().unwrap_or("(no title)");
            let url = result["url"].as_str().unwrap_or("(no url)");
            let description = result["description"].as_str().unwrap_or("");

            output.push_str(&format!(
                "[{}] {}\n    {}\n    {}\n",
                i + 1,
                title,
                url,
                description
            ));
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// WebFetchTool
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch URL and extract readable content (HTML -> text)."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Max chars to return",
                    "minimum": 100
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: url".into()))?;

        validate_url(url)?;

        let max_chars = params["max_chars"].as_u64().unwrap_or(12000) as usize;

        let client = reqwest::Client::builder()
            .user_agent("rustoctopus/0.1")
            .redirect(reqwest::redirect::Policy::limited(5))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to build HTTP client: {e}")))?;

        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Fetch failed: {e}")))?;

        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = resp
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response body: {e}")))?;

        let text = if content_type.contains("application/json") {
            // Pretty-print JSON
            match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(val) => serde_json::to_string_pretty(&val).unwrap_or(body),
                Err(_) => body,
            }
        } else if content_type.contains("text/html") || content_type.is_empty() {
            // HTML -> text extraction
            let stripped = strip_tags(&body);
            collapse_whitespace(&stripped)
        } else {
            body
        };

        let truncated = text.len() > max_chars;
        let text = if truncated {
            text[..max_chars].to_string()
        } else {
            text
        };

        let result = json!({
            "url": url,
            "status": status,
            "length": text.len(),
            "truncated": truncated,
            "text": text,
        });

        Ok(result.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_valid() {
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn test_validate_url_no_scheme() {
        assert!(validate_url("example.com").is_err());
    }

    #[test]
    fn test_validate_url_no_host() {
        assert!(validate_url("https://").is_err());
    }

    #[test]
    fn test_strip_tags_basic() {
        let result = strip_tags("<p>Hello <b>world</b></p>");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_strip_tags_script() {
        let result = strip_tags("<script>code</script>text");
        assert_eq!(result, "text");
    }

    #[test]
    fn test_collapse_whitespace() {
        let result = collapse_whitespace("  hello   world  \n\n  ");
        assert_eq!(result, "hello world");
    }

    #[tokio::test]
    async fn test_search_no_api_key() {
        let tool = WebSearchTool::new(None);
        let result = tool.execute(json!({"query": "test"})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Web search not configured: no Brave API key"),
            "Expected API key error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_fetch_invalid_url() {
        let tool = WebFetchTool::new();
        let result = tool.execute(json!({"url": "not-a-url"})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("URL must start with"),
            "Expected URL validation error, got: {}",
            err
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_search_live() {
        // Requires BRAVE_API_KEY environment variable
        let api_key = std::env::var("BRAVE_API_KEY").ok();
        let tool = WebSearchTool::new(api_key);
        let result = tool
            .execute(json!({"query": "rust programming language", "count": 3}))
            .await
            .unwrap();
        println!("{result}");
        assert!(!result.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_fetch_live() {
        let tool = WebFetchTool::new();
        let result = tool
            .execute(json!({"url": "https://example.com", "max_chars": 1000}))
            .await
            .unwrap();
        println!("{result}");
        assert!(result.contains("example"));
    }
}
