// commands.rs
pub fn extract_session_command(response_text: &str) -> Option<(String, String)> {
    for line in response_text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("SESSION:") {
            let rest = rest.trim();
            if let Some((session_name, command)) = rest.split_once(' ') {
                return Some((
                    session_name.trim().to_string(),
                    command.trim().to_string(),
                ));
            } else if !rest.is_empty() {
                return Some((rest.to_string(), String::new()));
            }
        }
    }
    None
}

pub fn extract_run_command(response_text: &str) -> Option<(String, String)> {
    for line in response_text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("TOOL_NAME: RUN") {
            let rest = rest.trim();
            if let Some(session_name) = rest.split_whitespace().next() {
                let command = rest.replacen(session_name, "", 1).trim().to_string();
                return Some((session_name.to_string(), command));
            }
        }
    }
    None
}

pub fn extract_end_command(response_text: &str) -> Option<String> {
    for line in response_text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("END_SESSION:") {
            return Some(name.trim().to_string());
        }
    }
    None
}

pub fn extract_command(response_text: &str) -> Option<String> {
    for line in response_text.lines() {
        let line = line.trim();
        if let Some(cmd) = line.strip_prefix("COMMAND:") {
            return Some(cmd.trim().to_string());
        }
    }
    None
}

/// Extracts JSON tool call content after "JSON_TOOL:" flag
pub fn extract_json_tool(response: &str) -> Option<String> {
    // Find the exact marker (case sensitive)
    let marker = "JSON_TOOL:";

    if let Some(start) = response.find(marker) {
        // Get everything after the marker
        let after_marker = &response[start + marker.len()..];

        // Skip any leading whitespace or newlines
        let trimmed = after_marker.trim_start();

        // Find the start of the actual JSON (first '{')
        if let Some(json_start) = trimmed.find('{') {
            let json_section = &trimmed[json_start..];

            // Count braces to find the end of the JSON object
            let mut depth = 0;
            let mut end_pos = 0;

            for (i, c) in json_section.char_indices() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = i + 1;
                        break;
                    }
                }
            }

            if end_pos > 0 {
                return Some(json_section[..end_pos].to_string());
            }
        }
    }

    None
}
