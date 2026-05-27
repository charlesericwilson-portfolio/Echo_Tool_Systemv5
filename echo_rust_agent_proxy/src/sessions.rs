use std::path::PathBuf;
use std::process::Command;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;
use serde_json::json;

use crate::summary::summarize_output;
use crate::log::save_chat_log_entry;
use crate::safety::is_command_safe;

pub async fn start_or_reuse_session(
    home_dir: PathBuf,
    active_sessions: &Arc<Mutex<HashMap<String, (String, String)>>>,
    name: &str,
    command: &str,
) -> Result<()> {
    let mut sessions = active_sessions.lock().await;
    sessions.insert(name.to_string(), (String::new(), String::new()));
    drop(sessions);

    // Check if session exists
    let check = Command::new("tmux")
        .args(["has-session", "-t", name])
        .status()?;

    if !check.success() {
        // Create new detached session
        Command::new("tmux")
            .args(["new-session", "-d", "-s", name])
            .current_dir(&home_dir)
            .status()?;
        println!("Created new tmux session: {}", name);
    } else {
        println!("Reusing existing tmux session: {}", name);
    }

    // Send the command
    Command::new("tmux")
        .args(["send-keys", "-t", name, command, "Enter"])
        .status()?;

    Ok(())
}

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

pub fn extract_end_command(response_text: &str) -> Option<String> {
    for line in response_text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("END_SESSION:") {
            return Some(name.trim().to_string());
        }
    }
    None
}

pub async fn execute_in_session(
    _home_dir: PathBuf,
    _active_sessions: &Arc<Mutex<HashMap<String, (String, String)>>>,
    name: &str,
    command: String,
) -> Result<String> {
    let marker_start = format!("===ECHO_START_{}===", chrono::Local::now().timestamp());
    let marker_end = format!("===ECHO_END_{}===", chrono::Local::now().timestamp());

    // Send markers and command
    Command::new("tmux")
        .args(["send-keys", "-t", name, &format!("echo '{}'", marker_start), "Enter"])
        .status()?;

    Command::new("tmux")
        .args(["send-keys", "-t", name, &command, "Enter"])
        .status()?;

    Command::new("tmux")
        .args(["send-keys", "-t", name, &format!("echo '{}'", marker_end), "Enter"])
        .status()?;

    // Wait a bit for command to run
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Capture the pane output
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-t", name])
        .output()?;

    let raw = String::from_utf8_lossy(&output.stdout).to_string();

    // Extract only the new output between markers
    let start_idx = raw.rfind(&marker_start).unwrap_or(0) + marker_start.len();
    let end_idx = raw.rfind(&marker_end).unwrap_or(raw.len());

    let clean_output = if start_idx < end_idx {
        raw[start_idx..end_idx].trim().to_string()
    } else {
        raw.trim().to_string()
    };

    Ok(clean_output)
}

pub async fn end_session(
    _home_dir: PathBuf,
    active_sessions: &Arc<Mutex<HashMap<String, (String, String)>>>,
    name: &str,
) -> Result<()> {
    let mut sessions = active_sessions.lock().await;
    sessions.remove(name);
    drop(sessions);

    let _ = Command::new("tmux").args(["kill-session", "-t", name]).status();
    Ok(())
}

pub async fn clean_up_sessions(
    _active_sessions: &Arc<Mutex<HashMap<String, (String, String)>>>
) -> Result<()> {
    // ... your existing code
    Ok(())
}

// === High-level handler that covers ALL session cases ===
pub async fn handle_session_command(
    agent: &mut crate::agent::EchoAgent,
    user_input: &str,
    session_name: &str,
    command: Option<&str>,
) -> Result<()> {
    //println!("{}Echo: {}", crate::agent::LIGHT_BLUE, current_response.trim());
    if let Some(cmd) = command {
        if let Err(e) = is_command_safe(cmd, &agent.config) {
            println!("{}Safety block: {}{}", crate::agent::YELLOW, e, crate::agent::RESET_COLOR);
            save_chat_log_entry(&agent.home_dir, user_input, &format!("Blocked: {}", e), "assistant").await?;
            agent.messages.push(json!({"role": "assistant", "content": format!("Safety block: {}", e)}));
        } else {
            start_or_reuse_session(agent.home_dir.clone(), &agent.active_sessions, session_name, cmd).await?;

            let raw_output = execute_in_session(agent.home_dir.clone(), &agent.active_sessions, session_name, cmd.to_string()).await?;

            let summary = match summarize_output(&raw_output, &agent.config).await {
                Ok(s) => s,
                Err(e) => format!("(Summarizer failed: {})", e),
            };

            agent.db.log_tool_call(session_name, cmd, &summary)?;

            let tool_content = format!("Tool output from SESSION '{}':\n{}", session_name, summary);
            println!("{}[Tool Summary]:\n{}{}", crate::agent::YELLOW, summary, crate::agent::RESET_COLOR);
            let tool_content_lower = tool_content.to_lowercase();
            // Append as assistant message (lowercase to avoid re-trigger)
            agent.messages.push(json!({"role": "assistant", "content": format!("Executed in session '{}'", session_name)}));
            agent.messages.push(json!({"role": "tool", "content": tool_content_lower}));
        }
    } else {
        // END_SESSION case
        let _ = end_session(agent.home_dir.clone(), &agent.active_sessions, session_name).await;
        let tool_content = format!("Session '{}' has been terminated.", session_name);
        agent.messages.push(json!({"role": "tool", "content": tool_content}));
    }

    Ok(())
}
