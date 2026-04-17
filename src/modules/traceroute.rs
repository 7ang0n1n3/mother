use anyhow::Result;
use std::net::IpAddr;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::OutputLine;
use super::ping::resolve;

/// Build a platform-appropriate traceroute command.
/// Returns (program, args).
fn traceroute_cmd(addr_str: &str) -> (&'static str, Vec<String>) {
    if cfg!(target_os = "windows") {
        // tracert -d (no DNS) -h 30 (max hops)
        ("tracert", vec!["-d".into(), "-h".into(), "30".into(), addr_str.to_string()])
    } else {
        // Linux & macOS: traceroute -n (no DNS) -m 30
        ("traceroute", vec!["-n".into(), "-m".into(), "30".into(), addr_str.to_string()])
    }
}

/// True if this output line is the tool's own header / summary and should be skipped.
fn is_header_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("traceroute to ")
        || lower.starts_with("tracing route to ")
        || lower.starts_with("trace complete")
        || lower.trim().is_empty()
}

pub async fn run(target: String, tx: mpsc::Sender<OutputLine>) -> Result<()> {
    let host = target.trim().to_string();
    if host.is_empty() {
        tx.send(OutputLine::Error("NO TARGET SPECIFIED.".into())).await.ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    }

    let addr = match resolve(&host).await {
        Some(IpAddr::V4(ip)) => ip,
        Some(IpAddr::V6(_)) => {
            tx.send(OutputLine::Error("IPV6 TRACEROUTE NOT SUPPORTED YET.".into()))
                .await
                .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
        None => {
            tx.send(OutputLine::Error(format!(
                "CANNOT RESOLVE: {}",
                host.to_uppercase()
            )))
            .await
            .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
    };

    tx.send(OutputLine::Bright(format!(
        "NETWORK PATH ANALYSIS: {} ({})",
        host.to_uppercase(),
        addr
    )))
    .await
    .ok();

    let addr_str = addr.to_string();
    let (prog, args) = traceroute_cmd(&addr_str);

    let mut child = match Command::new(prog)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            tx.send(OutputLine::Error(format!(
                "TRACEROUTE UNAVAILABLE — INSTALL {} TO USE THIS FEATURE.",
                prog.to_uppercase()
            )))
            .await
            .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if is_header_line(&line) {
            continue;
        }
        let upper = line.to_uppercase();
        if upper.contains(&addr_str.to_uppercase()) && !upper.contains('*') {
            tx.send(OutputLine::Bright(format!("  {}", upper.trim()))).await.ok();
        } else if upper.contains('*') {
            tx.send(OutputLine::Dim(format!("  {}", upper.trim()))).await.ok();
        } else {
            tx.send(OutputLine::Normal(format!("  {}", upper.trim()))).await.ok();
        }
    }

    child.wait().await.ok();

    tx.send(OutputLine::Dim("PATH ANALYSIS COMPLETE.".into())).await.ok();
    tx.send(OutputLine::Done).await.ok();
    Ok(())
}
