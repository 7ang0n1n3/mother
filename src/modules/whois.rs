use anyhow::Result;
use std::net::{IpAddr, SocketAddr};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

use super::OutputLine;

const WHOIS_PORT: u16 = 43;
const IANA_WHOIS: &str = "whois.iana.org";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const READ_TIMEOUT: Duration = Duration::from_secs(15);

/// Resolve `server` to an IPv4 address and connect — avoids ENETUNREACH on
/// systems without IPv6 routing when the hostname has AAAA records.
async fn resolve_ipv4(server: &str) -> Option<IpAddr> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(format!("{}:{}", server, WHOIS_PORT))
        .await
        .ok()?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .map(|a| a.ip())
}

async fn query_server(server: &str, query: &str) -> Result<Vec<String>> {
    let ip = resolve_ipv4(server)
        .await
        .ok_or_else(|| anyhow::anyhow!("cannot resolve {}", server))?;

    let sock_addr = SocketAddr::new(ip, WHOIS_PORT);
    let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(sock_addr)).await??;

    let (reader, mut writer) = tokio::io::split(stream);
    writer
        .write_all(format!("{}\r\n", query).as_bytes())
        .await?;
    writer.flush().await?;

    let mut lines = BufReader::new(reader).lines();
    let mut result = Vec::new();

    let read_fut = async {
        while let Ok(Some(line)) = lines.next_line().await {
            result.push(line);
        }
        Ok::<Vec<String>, anyhow::Error>(result)
    };

    timeout(READ_TIMEOUT, read_fut).await?
}

/// Returns true when an error string looks like a network-level block
/// (port 43 filtered, host unreachable, connection refused, or timeout).
fn is_connection_blocked(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("no route to host")
        || e.contains("network unreachable")
        || e.contains("connection refused")
        || e.contains("connection timed out")
        || e.contains("timed out")
        || e.contains("os error 101")  // ENETUNREACH
        || e.contains("os error 111")  // ECONNREFUSED
        || e.contains("os error 110")  // ETIMEDOUT
        || e.contains("os error 113")  // EHOSTUNREACH
}

/// Parse a referral WHOIS server from IANA or RIR response lines.
fn find_referral(lines: &[String]) -> Option<String> {
    for line in lines {
        let lower = line.to_lowercase();
        if let Some(rest) = lower.strip_prefix("whois:") {
            let s = rest.trim().to_string();
            if !s.is_empty() && s.contains('.') {
                return Some(s);
            }
        }
        if let Some(rest) = lower.strip_prefix("refer:") {
            let s = rest.trim().to_string();
            if !s.is_empty() && s.contains('.') {
                return Some(s);
            }
        }
    }
    None
}

/// Ordered list of servers to try for a given target, most-authoritative first.
fn candidate_servers(target: &str) -> Vec<&'static str> {
    if target.parse::<std::net::IpAddr>().is_ok() {
        return vec![
            "whois.arin.net",
            "whois.ripe.net",
            "whois.apnic.net",
            "whois.lacnic.net",
            "whois.afrinic.net",
        ];
    }
    let tld = target.rsplit('.').next().unwrap_or("").to_lowercase();
    match tld.as_str() {
        "com" | "net" | "edu" => vec!["whois.verisign-grs.com", "whois.internic.net"],
        "org"                 => vec!["whois.pir.org", "whois.publicinterestregistry.org"],
        "io"                  => vec!["whois.nic.io"],
        "uk"                  => vec!["whois.nic.uk"],
        "de"                  => vec!["whois.denic.de"],
        "fr"                  => vec!["whois.nic.fr"],
        "nl"                  => vec!["whois.domain-registry.nl"],
        "au"                  => vec!["whois.auda.org.au"],
        "ca"                  => vec!["whois.cira.ca"],
        "jp"                  => vec!["whois.jprs.jp"],
        "cn"                  => vec!["whois.cnnic.cn"],
        "br"                  => vec!["whois.registro.br"],
        "in"                  => vec!["whois.registry.in"],
        _                     => vec![IANA_WHOIS],
    }
}

pub async fn run(target: String, tx: mpsc::Sender<OutputLine>) -> Result<()> {
    let target = target.trim().to_string();
    if target.is_empty() {
        tx.send(OutputLine::Error("NO TARGET SPECIFIED.".into())).await.ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    }

    tx.send(OutputLine::Bright(format!(
        "REGISTRATION QUERY: {}",
        target.to_uppercase()
    )))
    .await
    .ok();

    // Step 1: try IANA for an authoritative referral.
    tx.send(OutputLine::Dim("  QUERYING IANA FOR REFERRAL...".into()))
        .await
        .ok();

    let referral: Option<String> = match query_server(IANA_WHOIS, &target).await {
        Ok(lines) => find_referral(&lines),
        Err(_) => None,
    };

    // Build the ordered server list: IANA referral (if any) followed by
    // built-in candidates for this TLD/IP.
    let mut servers: Vec<String> = Vec::new();
    if let Some(ref r) = referral {
        servers.push(r.clone());
    }
    for s in candidate_servers(&target) {
        let owned = s.to_string();
        if !servers.contains(&owned) {
            servers.push(owned);
        }
    }

    // Step 2: try each server in order, stop at the first success.
    let mut last_err = String::new();
    let mut any_connection_error = false;

    for server in &servers {
        tx.send(OutputLine::Dim(format!("  SERVER: {}", server.to_uppercase())))
            .await
            .ok();

        match query_server(server, &target).await {
            Ok(lines) => {
                emit_lines(&lines, &tx).await;
                tx.send(OutputLine::Dim("QUERY COMPLETE.".into())).await.ok();
                tx.send(OutputLine::Done).await.ok();
                return Ok(());
            }
            Err(e) => {
                last_err = e.to_string();
                if is_connection_blocked(&last_err) {
                    any_connection_error = true;
                    tx.send(OutputLine::Dim(format!(
                        "  {} UNREACHABLE — TRYING NEXT SERVER...",
                        server.to_uppercase()
                    )))
                    .await
                    .ok();
                } else {
                    // Protocol / non-connection error — report and stop.
                    tx.send(OutputLine::Error(format!(
                        "QUERY FAILED: {}",
                        last_err.to_uppercase()
                    )))
                    .await
                    .ok();
                    tx.send(OutputLine::Dim("QUERY COMPLETE.".into())).await.ok();
                    tx.send(OutputLine::Done).await.ok();
                    return Ok(());
                }
            }
        }
    }

    // All servers failed.
    if any_connection_error {
        tx.send(OutputLine::Error(
            "ALL WHOIS SERVERS UNREACHABLE — PORT 43 MAY BE BLOCKED BY YOUR NETWORK OR FIREWALL."
                .into(),
        ))
        .await
        .ok();
    } else {
        tx.send(OutputLine::Error(format!(
            "QUERY FAILED: {}",
            last_err.to_uppercase()
        )))
        .await
        .ok();
    }

    tx.send(OutputLine::Dim("QUERY COMPLETE.".into())).await.ok();
    tx.send(OutputLine::Done).await.ok();
    Ok(())
}

async fn emit_lines(lines: &[String], tx: &mpsc::Sender<OutputLine>) {
    for line in lines {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('%') || line.starts_with('#') {
            tx.send(OutputLine::Dim(format!("  {}", line.to_uppercase())))
                .await
                .ok();
        } else if line.contains(':') {
            tx.send(OutputLine::Normal(format!("  {}", line.to_uppercase())))
                .await
                .ok();
        } else {
            tx.send(OutputLine::Dim(format!("  {}", line.to_uppercase())))
                .await
                .ok();
        }
    }
}
