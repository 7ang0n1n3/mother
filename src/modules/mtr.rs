use anyhow::Result;
use std::net::IpAddr;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::OutputLine;
use super::ping::{parse_host_count, resolve};

pub async fn run(target: String, tx: mpsc::Sender<OutputLine>) -> Result<()> {
    let target = target.trim().to_string();
    if target.is_empty() {
        tx.send(OutputLine::Error("NO TARGET SPECIFIED.".into())).await.ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    }

    let (host, cycles_str) = parse_host_count(&target, "10");
    let cycles: u32 = cycles_str.parse().unwrap_or(10).max(1);

    let addr = match resolve(&host).await {
        Some(IpAddr::V4(ip)) => ip,
        Some(IpAddr::V6(_)) => {
            tx.send(OutputLine::Error("IPV6 MTR NOT SUPPORTED YET.".into()))
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
        "CONTINUOUS ROUTE ANALYSIS: {}  [{} CYCLES]",
        host.to_uppercase(),
        cycles
    )))
    .await
    .ok();
    tx.send(OutputLine::Dim("  RUNNING — THIS MAY TAKE SEVERAL SECONDS...".into()))
        .await
        .ok();

    let addr_str = addr.to_string();

    // Try mtr first; if it's not installed fall back to traceroute/tracert.
    match try_mtr(&addr_str, cycles, &tx).await {
        MtrResult::Ok => {}
        MtrResult::NotInstalled => {
            tx.send(OutputLine::Dim(
                "  MTR NOT FOUND — FALLING BACK TO SINGLE TRACEROUTE PATH.".into(),
            ))
            .await
            .ok();
            run_traceroute_fallback(&addr_str, &tx).await;
        }
    }

    tx.send(OutputLine::Dim("ANALYSIS COMPLETE.".into())).await.ok();
    tx.send(OutputLine::Done).await.ok();
    Ok(())
}

enum MtrResult {
    Ok,
    NotInstalled,
}

async fn try_mtr(addr_str: &str, cycles: u32, tx: &mpsc::Sender<OutputLine>) -> MtrResult {
    let mut child = match Command::new("mtr")
        .args([
            "--report",
            "--no-dns",
            "--report-cycles",
            &cycles.to_string(),
            addr_str,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return MtrResult::NotInstalled,
    };

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    // Skip "Start: ..." line; emit "HOST: ..." column header as dim.
    let mut skipped = 0usize;
    while let Ok(Some(line)) = lines.next_line().await {
        if skipped < 2 {
            skipped += 1;
            if skipped == 2 {
                tx.send(OutputLine::Dim(format!("  {}", line.trim().to_uppercase())))
                    .await
                    .ok();
            }
            continue;
        }

        let trimmed = line.trim().to_uppercase();
        if trimmed.is_empty() {
            continue;
        }

        let out = if trimmed.contains("100.0%") {
            OutputLine::Dim(format!("  {}", trimmed))
        } else if trimmed.contains(" 0.0%") {
            OutputLine::Normal(format!("  {}", trimmed))
        } else {
            OutputLine::Error(format!("  {}", trimmed))
        };
        tx.send(out).await.ok();
    }

    child.wait().await.ok();
    MtrResult::Ok
}

async fn run_traceroute_fallback(addr_str: &str, tx: &mpsc::Sender<OutputLine>) {
    let (prog, args): (&str, Vec<String>) = if cfg!(target_os = "windows") {
        ("tracert", vec!["-d".into(), "-h".into(), "30".into(), addr_str.to_string()])
    } else {
        ("traceroute", vec!["-n".into(), "-m".into(), "30".into(), addr_str.to_string()])
    };

    let mut child = match Command::new(prog)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            tx.send(OutputLine::Error(format!(
                "  NO TRACEROUTE TOOL FOUND. INSTALL MTR OR {}.",
                prog.to_uppercase()
            )))
            .await
            .ok();
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let lower = line.to_lowercase();
        if lower.starts_with("traceroute to ")
            || lower.starts_with("tracing route to ")
            || lower.starts_with("trace complete")
            || line.trim().is_empty()
        {
            continue;
        }
        let upper = line.to_uppercase();
        if upper.contains('*') {
            tx.send(OutputLine::Dim(format!("  {}", upper.trim()))).await.ok();
        } else {
            tx.send(OutputLine::Normal(format!("  {}", upper.trim()))).await.ok();
        }
    }

    child.wait().await.ok();
}
