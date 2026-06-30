use std::{process::ExitStatus, process::Output};

pub(crate) const TOOL_LOG_TAIL_BYTES: u64 = 4096;

pub fn command_output_diagnostic_summary(output: &Output) -> String {
    let mut parts = vec![exit_status_summary(output.status)];
    if !output.stdout.is_empty() {
        parts.push(format!(
            "stdout_tail=\"{}\"",
            escaped_byte_tail(&output.stdout, TOOL_LOG_TAIL_BYTES as usize)
        ));
    }
    if !output.stderr.is_empty() {
        parts.push(format!(
            "stderr_tail=\"{}\"",
            escaped_byte_tail(&output.stderr, TOOL_LOG_TAIL_BYTES as usize)
        ));
    }
    parts.join(" ")
}

fn exit_status_summary(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit_code={code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal={signal}");
        }
    }
    "signal".to_string()
}

fn escaped_byte_tail(bytes: &[u8], max_bytes: usize) -> String {
    let start = bytes.len().saturating_sub(max_bytes);
    escape_log_field(&String::from_utf8_lossy(&bytes[start..]))
}

pub(crate) fn escape_log_field(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other if other.is_control() => "?".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}
