use miniserde::{Deserialize, Serialize, json};
use std::process::Stdio;
use std::time::Duration;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const EXEC_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteExec {
    message: String,
    task_id: String,
    command: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteExecCallback {
    task_id: String,
    result: String,
    exit_code: i32,
    finished_at: String,
}

pub async fn exec_command(
    utf8_str: &str,
    callback_url: String,
    terminal_entry: &str,
    ignore_unsafe_cert: bool,
) -> Result<(), String> {
    let remote_exec: RemoteExec =
        json::from_str(utf8_str).map_err(|_| "Failed to parse RemoteExec".to_string())?;
    let terminal_entry = terminal_entry.to_string();

    let exec = tokio::spawn(async move {
        let (shell, shell_arg) = if cfg!(windows) {
            ("cmd.exe", "/C")
        } else {
            let entry = terminal_entry.trim();
            if entry.is_empty() || entry == "default" {
                ("sh", "-c")
            } else {
                // Reuse user-selected terminal entry to keep command execution behavior aligned.
                (entry, "-c")
            }
        };

        let mut child = Command::new(shell)
            .arg(shell_arg)
            .arg(&remote_exec.command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to execute process: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "failed to capture stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "failed to capture stderr".to_string())?;

        let stdout_task = tokio::spawn(read_limited_stream(stdout));
        let stderr_task = tokio::spawn(read_limited_stream(stderr));

        let (status_code, timed_out) = match tokio::time::timeout(EXEC_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) => (status.code().unwrap_or(1), false),
            Ok(Err(e)) => return Err(format!("failed to wait process: {e}")),
            Err(_) => {
                // Kill timed-out commands to prevent runaway subprocess resource usage.
                let _ = child.kill().await;
                let code = child
                    .wait()
                    .await
                    .ok()
                    .and_then(|status| status.code())
                    .unwrap_or(124);
                (code, true)
            }
        };

        let stdout_bytes = stdout_task
            .await
            .map_err(|e| format!("failed to join stdout reader: {e}"))?;
        let stderr_bytes = stderr_task
            .await
            .map_err(|e| format!("failed to join stderr reader: {e}"))?;

        let stdout_str = String::from_utf8_lossy(&stdout_bytes);
        let stderr_str = String::from_utf8_lossy(&stderr_bytes);

        let mut output = format!("{stdout_str}{stderr_str}");
        if timed_out {
            output.push_str("\n[command timed out after 30 seconds]");
        }

        Ok((status_code, limit_output_size(&output)))
    });

    let Ok(Ok((status, output))) = exec.await else {
        return Err("failed to execute process".to_string());
    };

    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let finished_at = now.format(&Rfc3339).unwrap_or_default();

    let reply = RemoteExecCallback {
        task_id: remote_exec.task_id,
        result: output,
        exit_code: status,
        finished_at,
    };

    let json_string = json::to_string(&reply);
    tokio::task::spawn_blocking(move || {
        send_exec_callback_blocking(&callback_url, &json_string, ignore_unsafe_cert)
    })
    .await
    .map_err(|e| format!("failed to join callback sender: {e}"))?
}

fn send_exec_callback_blocking(
    callback_url: &str,
    payload: &str,
    ignore_unsafe_cert: bool,
) -> Result<(), String> {
    #[cfg(feature = "ureq-support")]
    {
        use crate::utils::create_ureq_agent;
        let agent = create_ureq_agent(ignore_unsafe_cert);
        if let Ok(req) = agent.post(callback_url).send(payload) {
            if req.status().is_success() {
                return Ok(());
            }
            return Err("server returned a error".to_string());
        }
        return Err("Unable to connect server".to_string());
    }

    #[cfg(all(not(feature = "ureq-support"), feature = "nyquest-support"))]
    {
        use nyquest::Body;
        use nyquest::Request;
        let client = crate::utils::create_nyquest_client(ignore_unsafe_cert);
        let body = Body::text(payload.to_string(), "application/json");
        let request = Request::post(callback_url.to_string()).with_body(body);

        if let Ok(res) = client.request(request) {
            if res.status().is_successful() {
                return Ok(());
            }
            return Err("server returned a error".to_string());
        }
        return Err("Unable to connect server".to_string());
    }

    #[allow(unreachable_code)]
    Err("No HTTP backend enabled".to_string())
}

async fn read_limited_stream<R>(mut reader: R) -> Vec<u8>
where
    R: AsyncRead + Unpin,
{
    let mut buf = [0_u8; 4096];
    let mut collected = Vec::with_capacity(4096);

    loop {
        let count = match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(count) => count,
            Err(_) => break,
        };

        if collected.len() < MAX_OUTPUT_BYTES {
            let remain = MAX_OUTPUT_BYTES - collected.len();
            let take = remain.min(count);
            collected.extend_from_slice(&buf[..take]);
        }
    }

    collected
}

fn limit_output_size(output: &str) -> String {
    if output.len() <= MAX_OUTPUT_BYTES {
        return output.to_string();
    }

    // Trim at char boundary to avoid invalid UTF-8 after truncation.
    let mut end = MAX_OUTPUT_BYTES;
    while !output.is_char_boundary(end) {
        end -= 1;
    }

    let mut truncated = output[..end].to_string();
    truncated.push_str("\n[output truncated]");
    truncated
}
