use anyhow::Result;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use surge_ping::{Client, Config, IcmpPacket, PingIdentifier, PingSequence, ICMP};
use tokio::sync::mpsc;

use super::OutputLine;

pub async fn run(target: String, tx: mpsc::Sender<OutputLine>) -> Result<()> {
    let target = target.trim().to_string();
    if target.is_empty() {
        tx.send(OutputLine::Error("NO TARGET SPECIFIED.".into())).await.ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    }

    let (host, count_str) = parse_host_count(&target, "4");
    let count: u16 = count_str.parse().unwrap_or(4);

    tx.send(OutputLine::Bright(format!(
        "ICMP PROBE: {}  [{} PACKETS]",
        host.to_uppercase(),
        count
    )))
    .await
    .ok();

    let addr = match resolve(&host).await {
        Some(a) => a,
        None => {
            tx.send(OutputLine::Error(format!(
                "CANNOT RESOLVE HOST: {}",
                host.to_uppercase()
            )))
            .await
            .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
    };

    // Show resolved IP if host was a name
    if host.parse::<IpAddr>().is_err() {
        tx.send(OutputLine::Dim(format!("  RESOLVED: {}", addr)))
            .await
            .ok();
    }

    let kind = match addr {
        IpAddr::V4(_) => ICMP::V4,
        IpAddr::V6(_) => ICMP::V6,
    };

    let client = match Client::new(&Config::builder().kind(kind).build()) {
        Ok(c) => c,
        Err(e) => {
            tx.send(OutputLine::Error(format!(
                "SOCKET ERROR: {} (RUN AS ROOT/ADMIN?)",
                e.to_string().to_uppercase()
            )))
            .await
            .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
    };

    let id = PingIdentifier(std::process::id() as u16);
    let mut pinger = client.pinger(addr, id).await;
    pinger.timeout(Duration::from_secs(2));

    let payload = vec![0u8; 56];
    let mut received: u16 = 0;
    let mut total_ms: f64 = 0.0;

    for seq in 0..count {
        match pinger.ping(PingSequence(seq), &payload).await {
            Ok((packet, rtt)) => {
                received += 1;
                let ms = rtt.as_secs_f64() * 1000.0;
                total_ms += ms;
                let src = match &packet {
                    IcmpPacket::V4(_) => addr.to_string(),
                    IcmpPacket::V6(_) => addr.to_string(),
                };
                tx.send(OutputLine::Normal(format!(
                    "  64 BYTES FROM {}: ICMP_SEQ={} TIME={:.2}MS",
                    src.to_uppercase(),
                    seq,
                    ms
                )))
                .await
                .ok();
            }
            Err(e) => {
                tx.send(OutputLine::Error(format!(
                    "  SEQ={} {}",
                    seq,
                    e.to_string().to_uppercase()
                )))
                .await
                .ok();
            }
        }
        if seq + 1 < count {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    let loss_pct = if count > 0 {
        (count - received) * 100 / count
    } else {
        100
    };
    let avg_ms = if received > 0 {
        total_ms / received as f64
    } else {
        0.0
    };

    tx.send(OutputLine::Normal(format!(
        "  --- {} PING STATISTICS ---",
        host.to_uppercase()
    )))
    .await
    .ok();
    tx.send(OutputLine::Normal(format!(
        "  {}/{} PACKETS  {}% LOSS  AVG {:.2}MS",
        received, count, loss_pct, avg_ms
    )))
    .await
    .ok();

    tx.send(OutputLine::Dim("PROBE COMPLETE.".into())).await.ok();
    tx.send(OutputLine::Done).await.ok();
    Ok(())
}

/// Resolve a hostname or IP string to an IpAddr.
pub async fn resolve(host: &str) -> Option<IpAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(format!("{}:0", host))
        .await
        .ok()?
        .collect();
    addrs.first().map(|s| s.ip())
}

/// Split "host [count]" — trailing numeric token is treated as count.
pub fn parse_host_count(input: &str, default_count: &str) -> (String, String) {
    if let Some(idx) = input.rfind(' ') {
        let suffix = input[idx + 1..].trim();
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return (input[..idx].trim().to_string(), suffix.to_string());
        }
    }
    (input.trim().to_string(), default_count.to_string())
}
