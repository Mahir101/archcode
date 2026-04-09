use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::event::Event;
use super::manager::{Tool, ToolDefinition, ToolResult};

pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "WebSearch".into(),
            description: "Search the web for a query. Returns a summary of results.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query." }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let query = match args["query"].as_str() {
            Some(q) => q.to_string(),
            None => return Ok(ToolResult::err("Missing 'query'")),
        };

        // Uses DuckDuckGo Lite (no API key required)
        let url = format!(
            "https://lite.duckduckgo.com/lite/?q={}",
            urlencoding_simple(&query)
        );

        let client = reqwest::Client::builder()
            .user_agent("archcode/0.1 (search)")
            .build()?;

        let resp = client.get(&url).send().await
            .map_err(|e| anyhow::anyhow!("Search request failed: {e}"))?;

        let body = resp.text().await?;

        // Very simple extraction — strip HTML tags, take first 2000 chars
        let plain = strip_html(&body);
        let truncated = if plain.len() > 2000 {
            format!("{}...", &plain[..2000])
        } else {
            plain
        };

        Ok(ToolResult::ok(truncated))
    }
}

fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => '+'.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

fn strip_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    // Collapse whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}
