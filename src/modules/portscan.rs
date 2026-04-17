use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::timeout;

use super::OutputLine;

fn service_name(port: u16) -> &'static str {
    match port {
        20    => "FTP-DATA",
        21    => "FTP",
        22    => "SSH",
        23    => "TELNET",
        25    => "SMTP",
        53    => "DNS",
        67    => "DHCP",
        80    => "HTTP",
        110   => "POP3",
        111   => "RPCBIND",
        143   => "IMAP",
        389   => "LDAP",
        443   => "HTTPS",
        445   => "SMB",
        465   => "SMTPS",
        587   => "SUBMISSION",
        631   => "IPP",
        993   => "IMAPS",
        995   => "POP3S",
        1433  => "MSSQL",
        1521  => "ORACLE",
        2049  => "NFS",
        2375  => "DOCKER",
        3000  => "DEV-SERVER",
        3306  => "MYSQL",
        3389  => "RDP",
        4444  => "METASPLOIT",
        5000  => "FLASK",
        5432  => "POSTGRESQL",
        5900  => "VNC",
        6379  => "REDIS",
        6443  => "K8S-API",
        8080  => "HTTP-ALT",
        8443  => "HTTPS-ALT",
        8888  => "JUPYTER",
        9200  => "ELASTICSEARCH",
        9300  => "ES-CLUSTER",
        27017 => "MONGODB",
        _     => "",
    }
}

fn parse_ports(spec: &str) -> Result<Vec<u16>> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok((1u16..=1024).collect());
    }
    if let Some((lo_s, hi_s)) = spec.split_once('-') {
        let lo: u16 = lo_s.trim().parse()?;
        let hi: u16 = hi_s.trim().parse()?;
        return Ok((lo..=hi).collect());
    }
    // comma-separated or single port
    let ports: Result<Vec<u16>, _> = spec.split(',').map(|p| p.trim().parse::<u16>()).collect();
    Ok(ports?)
}

pub async fn run(target: String, tx: mpsc::Sender<OutputLine>) -> Result<()> {
    let target = target.trim().to_string();
    if target.is_empty() {
        tx.send(OutputLine::Error("NO TARGET SPECIFIED.".into())).await.ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    }

    let (host, port_spec) = match target.split_once(' ') {
        Some((h, p)) => (h.trim().to_string(), p.trim().to_string()),
        None => (target.clone(), String::new()),
    };

    let ports = match parse_ports(&port_spec) {
        Ok(p) => p,
        Err(e) => {
            tx.send(OutputLine::Error(
                format!("PORT SPEC INVALID: {}", e.to_string().to_uppercase()),
            ))
            .await
            .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
    };

    let n = ports.len();
    tx.send(OutputLine::Bright(format!(
        "PORT SCAN: {}  [{} PORTS]",
        host.to_uppercase(),
        n
    )))
    .await
    .ok();
    tx.send(OutputLine::Dim(
        "  PORT    STATE    SERVICE".into(),
    ))
    .await
    .ok();

    // Resolve hostname — prefer IPv4 to avoid ENETUNREACH on no-IPv6 systems
    let base_ip: IpAddr = match resolve_ipv4(&host).await {
        Some(ip) => ip,
        None => {
            tx.send(OutputLine::Error(format!(
                "DNS RESOLUTION FAILED: {}",
                host.to_uppercase()
            )))
            .await
            .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
    };

    let sem     = Arc::new(Semaphore::new(500));
    let mut handles: Vec<tokio::task::JoinHandle<Option<u16>>> =
        Vec::with_capacity(ports.len());

    for port in ports {
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let addr    = SocketAddr::new(base_ip, port);
            let result  = timeout(Duration::from_millis(500), TcpStream::connect(addr)).await;
            if let Ok(Ok(_)) = result { Some(port) } else { None }
        }));
    }

    // Collect all results then sort so output is always in port order
    let mut open_ports: Vec<u16> = Vec::new();
    for handle in handles {
        if let Ok(Some(port)) = handle.await {
            open_ports.push(port);
        }
    }
    open_ports.sort_unstable();

    for port in &open_ports {
        let svc = service_name(*port);
        let svc_str = if svc.is_empty() {
            String::new()
        } else {
            format!("  {}", svc)
        };
        tx.send(OutputLine::Bright(format!(
            "  {:5}/TCP  OPEN{}",
            port, svc_str
        )))
        .await
        .ok();
    }

    tx.send(OutputLine::Dim(format!(
        "SCAN COMPLETE. {} OPEN PORT(S) DETECTED.",
        open_ports.len()
    )))
    .await
    .ok();
    tx.send(OutputLine::Done).await.ok();
    Ok(())
}

/// Resolve a hostname, preferring IPv4 to avoid ENETUNREACH on no-IPv6 systems.
async fn resolve_ipv4(host: &str) -> Option<IpAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(format!("{}:0", host))
        .await
        .ok()?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .map(|a| a.ip())
}
