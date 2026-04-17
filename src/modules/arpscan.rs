use anyhow::Result;
use ipnetwork::Ipv4Network;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::timeout;

use super::OutputLine;

const MAX_PREFIX_SCAN: u8 = 16;
const PING_CONCURRENCY: usize = 128;
const PROBE_TIMEOUT_MS: u64 = 500;

/// Read the kernel ARP cache from /proc/net/arp.
/// Returns a map of IPv4 address → MAC string for completed entries only.
fn read_arp_cache() -> BTreeMap<Ipv4Addr, String> {
    let mut map = BTreeMap::new();
    let Ok(contents) = std::fs::read_to_string("/proc/net/arp") else {
        return map;
    };
    for line in contents.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        // cols: [ip, hw_type, flags, hw_addr, mask, device]
        let flags = u32::from_str_radix(cols[2].trim_start_matches("0x"), 16).unwrap_or(0);
        // ATF_COM (0x2): entry is complete — has a valid MAC
        if flags & 0x2 == 0 {
            continue;
        }
        let mac = cols[3].to_string();
        if mac == "00:00:00:00:00:00" {
            continue;
        }
        if let Ok(ip) = cols[0].parse::<Ipv4Addr>() {
            map.insert(ip, mac);
        }
    }
    map
}

/// Probe a single host by attempting a non-blocking TCP connect to port 7 (echo)
/// or port 80 — the actual connection doesn't need to succeed; the kernel ARP
/// resolution happens on the first packet regardless.
async fn probe_host(ip: Ipv4Addr) {
    let addr = SocketAddr::new(IpAddr::V4(ip), 80);
    let _ = timeout(
        Duration::from_millis(PROBE_TIMEOUT_MS),
        TcpStream::connect(addr),
    )
    .await;
}

pub async fn run(target: String, tx: mpsc::Sender<OutputLine>) -> Result<()> {
    let target = target.trim().to_string();
    let label = if target.is_empty() {
        "LOCAL NETWORK".to_string()
    } else {
        target.to_uppercase()
    };

    tx.send(OutputLine::Bright(format!("ARP SCAN: {}", label)))
        .await
        .ok();

    // Determine the subnet to probe
    let scan_net: Ipv4Network = if target.is_empty() {
        // Find the default local subnet from system interfaces
        match local_subnet() {
            Some(net) => net,
            None => {
                tx.send(OutputLine::Error(
                    "CANNOT DETERMINE LOCAL SUBNET.".into(),
                ))
                .await
                .ok();
                tx.send(OutputLine::Done).await.ok();
                return Ok(());
            }
        }
    } else if let Ok(net) = target.parse::<Ipv4Network>() {
        net
    } else if let Ok(ip) = target.parse::<Ipv4Addr>() {
        Ipv4Network::new(ip, 32).unwrap()
    } else {
        tx.send(OutputLine::Error(format!(
            "INVALID TARGET: {} (USE IP OR CIDR)",
            target.to_uppercase()
        )))
        .await
        .ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    };

    if scan_net.prefix() < MAX_PREFIX_SCAN {
        tx.send(OutputLine::Error(format!(
            "SUBNET {} TOO LARGE (MIN /{}) — SPECIFY A CIDR TARGET.",
            scan_net, MAX_PREFIX_SCAN
        )))
        .await
        .ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    }

    let hosts: Vec<Ipv4Addr> = scan_net
        .iter()
        .filter(|ip| *ip != scan_net.network() && *ip != scan_net.broadcast())
        .collect();

    tx.send(OutputLine::Dim(format!(
        "  PROBING {} HOSTS IN {} — PLEASE WAIT...",
        hosts.len(),
        scan_net
    )))
    .await
    .ok();

    // Snapshot the ARP cache before probing so we can tell what's new
    let before = read_arp_cache();

    // Concurrently probe all hosts to trigger ARP resolution
    let sem = Arc::new(Semaphore::new(PING_CONCURRENCY));
    let mut handles = Vec::with_capacity(hosts.len());
    for ip in hosts {
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            probe_host(ip).await;
        }));
    }
    for h in handles {
        let _ = h.await;
    }

    // Brief extra wait for late ARP replies to arrive
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Read the refreshed ARP cache
    let after = read_arp_cache();

    // Merge: prefer newly-discovered entries, but also show pre-existing ones
    let mut combined: BTreeMap<Ipv4Addr, String> = before;
    for (ip, mac) in after {
        combined.insert(ip, mac);
    }

    // Filter to only hosts within the target subnet
    let discovered: Vec<_> = combined
        .iter()
        .filter(|(ip, _)| scan_net.contains(**ip))
        .collect();

    if discovered.is_empty() {
        tx.send(OutputLine::Dim("  NO HOSTS DISCOVERED.".into()))
            .await
            .ok();
    } else {
        for (ip, mac) in &discovered {
            tx.send(OutputLine::Bright(
                format!("  {}  {}", ip, mac).to_uppercase(),
            ))
            .await
            .ok();
        }
    }

    tx.send(OutputLine::Dim("DISCOVERY COMPLETE.".into())).await.ok();
    tx.send(OutputLine::Done).await.ok();
    Ok(())
}

/// Find the first non-loopback IPv4 network from system interfaces.
fn local_subnet() -> Option<Ipv4Network> {
    use pnet::datalink;
    for iface in datalink::interfaces() {
        if !iface.is_up() || iface.is_loopback() {
            continue;
        }
        for net in &iface.ips {
            if let ipnetwork::IpNetwork::V4(v4) = net {
                if v4.prefix() >= MAX_PREFIX_SCAN {
                    if let Ok(anchored) = Ipv4Network::new(v4.network(), v4.prefix()) {
                        return Some(anchored);
                    }
                }
            }
        }
    }
    None
}
