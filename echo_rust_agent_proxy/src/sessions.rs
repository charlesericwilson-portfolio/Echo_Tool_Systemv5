// sessions.rs
use anyhow::{bail, Result};
use std::path::PathBuf;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

pub use crate::ACTIVE_SESSIONS;

/// Start or reuse a tmux session
/// Start or reuse a tmux session
/// Start or reuse a tmux session with a persistent shell
pub async fn start_or_reuse_session(_home: PathBuf, name: &str, _initial_command: &str) -> Result<()> {
    let exists = Command::new("tmux")
        .args(["has-session", "-t", name])
        .output().await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if exists {
        println!("Echo: Reusing existing tmux session '{}'.", name);
        return Ok(());
    }

    println!("Echo: Creating new tmux session '{}'", name);

    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", name, "bash -i"])
        .status().await?;

    if !status.success() {
        bail!("Failed to create tmux session '{}'", name);
    }

    let mut sessions = ACTIVE_SESSIONS.lock().await;
    sessions.insert(name.to_string(), (String::new(), String::new()));

    sleep(Duration::from_millis(600)).await;

    Ok(())
}

/// Send command to tmux session and return ONLY the new output (cleaned)
pub async fn execute_in_session(_home: PathBuf, session_name: &str, command: String) -> Result<String> {
    let start_marker = format!("===ECHO_START_{}===", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis());

    let end_marker = format!("===ECHO_END_{}===", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() + 1);

    // Send command with markers
    let full_input = format!(
        "echo '{}'\n{}\necho '{}'",
        start_marker, command, end_marker
    );

    let _ = Command::new("tmux")
        .args(["send-keys", "-t", session_name, &full_input, "Enter"])
        .status().await;

    // Poll for the end marker instead of sleeping a fixed time
    let _output = String::new();
    let start_time = std::time::Instant::now();
    let timeout = Duration::from_secs(60); // safety timeout

    loop {
        if start_time.elapsed() > timeout {
            return Ok("Command timed out".to_string());
        }

        let capture = Command::new("tmux")
            .args(["capture-pane", "-p", "-S", "-500", "-t", session_name])
            .output().await?;

        let raw = String::from_utf8_lossy(&capture.stdout).to_string();

        if let Some(start_pos) = raw.rfind(&start_marker) {
            let after_start = &raw[start_pos + start_marker.len()..];
            if let Some(end_pos) = after_start.find(&end_marker) {
                let fresh_output = &after_start[0..end_pos];
                let cleaned: String = fresh_output.lines()
                    .filter(|line| {
                        !line.trim().is_empty()
                            && !line.trim().ends_with('$')
                            && !line.trim().starts_with("eric@")
                            && !line.trim().starts_with("echo '===ECHO_")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return Ok(cleaned.trim().to_string());
            }
        }

        sleep(Duration::from_millis(300)).await; // poll every 300ms
    }
}
/// End / kill a tmux session gracefully
pub async fn end_session(_home_dir: PathBuf, name: &str) -> Result<()> {
    let mut sessions = ACTIVE_SESSIONS.lock().await;

    if sessions.remove(name).is_some() {
        println!("Echo: Terminating tmux session '{}'.", name);

        // Send Ctrl+C first for graceful shutdown
        let _ = Command::new("tmux").args(["send-keys", "-t", name, "C-c"]).status().await;
        sleep(Duration::from_millis(600)).await;

        // Kill the session
        let _ = Command::new("tmux").args(["kill-session", "-t", name]).status().await;

        Ok(())
    } else {
        bail!("Session '{}' not active.", name);
    }
}

/// Clean up all sessions on exit
pub async fn clean_up_sessions() -> Result<()> {
    let mut sessions = ACTIVE_SESSIONS.lock().await;
    let names: Vec<String> = sessions.keys().cloned().collect();

    for name in names {
        println!("Echo: Cleaning up tmux session '{}'.", name);
        let _ = Command::new("tmux").args(["kill-session", "-t", &name]).status().await;
    }

    sessions.clear();
    Ok(())
}
