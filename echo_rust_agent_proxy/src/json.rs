use serde_json::Value;
use anyhow::Result;
use chrono::Local;
use crate::log::save_chat_log_entry;

pub async fn handle_json_tool_call_str(tool_call: &str, web_search_url: Option<&str>) -> Result<String> {
    let parsed: Value = serde_json::from_str(tool_call)
        .map_err(|e| anyhow::anyhow!("Failed to parse JSON tool call: {}", e))?;

    // Support the format the model is actually outputting
    let function = if parsed["tool_calls"].is_array() && parsed["tool_calls"][0]["function"].is_object() {
        &parsed["tool_calls"][0]["function"]
    } else if parsed["function"].is_object() {
        &parsed["function"]
    } else {
        &parsed
    };

    let tool_name = function["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No tool name found in JSON"))?;

    let arguments: Value = if function["arguments"].is_string() {
        let args_str = function["arguments"].as_str().unwrap();
        serde_json::from_str(args_str).unwrap_or(Value::Object(serde_json::Map::new()))
    } else if function["arguments"].is_object() {
        function["arguments"].clone()
    } else {
        Value::Object(serde_json::Map::new())
    };

    match tool_name {
        "get_current_datetime" => {
            let now = Local::now();
            Ok(format!("Current datetime: {}", now.format("%Y-%m-%d %H:%M:%S %Z")))
        }

        "web_search" => {
            let query = arguments["query"].as_str().unwrap_or("No query provided");

            if let Some(url) = web_search_url {
                Ok(format!("Web search results for '{}':\n[Would call: {}?q={}]", query, url, query))
            } else {
                Ok(format!("Web search not configured. Query was: {}", query))
            }
        }

        _ => Err(anyhow::anyhow!("Unknown JSON tool: {}", tool_name)),
    }
}

pub async fn handle_json_tool(
    agent: &mut crate::agent::EchoAgent,
    user_input: &str,
    _current_response: &str,
    json_content: &str,
) -> Result<()> {
    println!("{}Echo: Detected JSON tool call{}", crate::agent::YELLOW, crate::agent::RESET_COLOR);

    // Pull the web search URL from config so the field is actually used
    let web_search_url = agent.config.web_search.as_ref().map(|w| w.url.as_str());

    match handle_json_tool_call_str(json_content, web_search_url).await {
        Ok(result) => {
            let tool_content = format!("Tool output:\n{}", result);
            save_chat_log_entry(&agent.home_dir, user_input, &tool_content, "assistant").await?;
            agent.messages.push(serde_json::json!({"role": "tool", "content": tool_content}));
        }
        Err(e) => {
            let error_msg = format!("JSON Tool error: {}", e);
            agent.messages.push(serde_json::json!({"role": "tool", "content": error_msg}));
        }
    }

    Ok(())
}

/// Extracts JSON tool call content from <json>...</json> tags
pub fn extract_json_tool(response: &str) -> Option<String> {
    if let Some(start) = response.find("<json>") {
        if let Some(end) = response[start..].find("</json>") {
            let inner = &response[start + 6..start + end];
            return Some(inner.trim().to_string());
        }
    }
    None
}
