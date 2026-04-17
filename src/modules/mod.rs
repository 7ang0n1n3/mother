pub mod arpscan;
pub mod dns;
pub mod mtr;
pub mod ping;
pub mod portscan;
pub mod traceroute;
pub mod whois;

use tokio::sync::mpsc;

// ── Output line variants sent from scan tasks to the UI ──────────────────────

pub enum OutputLine {
    Normal(String),  // standard output — GREEN_NORMAL
    Bright(String),  // open ports, success, headers — GREEN_BRIGHT
    Dim(String),     // separators, timestamps — GREEN_DIM
    Error(String),   // failures, warnings — GREEN_BRIGHT + BOLD + BLINK
    Done,            // sentinel: scan complete; never added to output vec
}

// ── Static module descriptors ────────────────────────────────────────────────

pub struct ModuleInfo {
    pub name:        &'static str,
    pub description: &'static str,
    pub hint:        &'static str,
}

pub const MODULES: &[ModuleInfo] = &[
    ModuleInfo {
        name:        "PORT SCAN",
        description: "ASYNC TCP CONNECT SCAN",
        hint:        "HOST [PORT_SPEC]  e.g. 192.168.1.1 1-1024",
    },
    ModuleInfo {
        name:        "PING",
        description: "ICMP ECHO PROBE",
        hint:        "HOST [COUNT]  e.g. 8.8.8.8 5",
    },
    ModuleInfo {
        name:        "TRACEROUTE",
        description: "NETWORK PATH ANALYSIS",
        hint:        "HOST  e.g. 8.8.8.8",
    },
    ModuleInfo {
        name:        "DNS LOOKUP",
        description: "DNS RESOURCE RECORD QUERY",
        hint:        "DOMAIN [TYPE]  e.g. example.com MX",
    },
    ModuleInfo {
        name:        "ARP SCAN",
        description: "LOCAL NETWORK DISCOVERY",
        hint:        "NETWORK/CIDR  e.g. 192.168.1.0/24",
    },
    ModuleInfo {
        name:        "WHOIS",
        description: "DOMAIN/IP REGISTRATION",
        hint:        "DOMAIN OR IP  e.g. example.com",
    },
    ModuleInfo {
        name:        "MTR",
        description: "CONTINUOUS ROUTE ANALYSIS",
        hint:        "HOST [CYCLES]  e.g. 8.8.8.8 10",
    },
];

// ── Dispatch ─────────────────────────────────────────────────────────────────

pub async fn run_module(
    idx:    usize,
    target: String,
    tx:     mpsc::Sender<OutputLine>,
) -> anyhow::Result<()> {
    match idx {
        0 => portscan::run(target, tx).await,
        1 => ping::run(target, tx).await,
        2 => traceroute::run(target, tx).await,
        3 => dns::run(target, tx).await,
        4 => arpscan::run(target, tx).await,
        5 => whois::run(target, tx).await,
        6 => mtr::run(target, tx).await,
        _ => {
            tx.send(OutputLine::Error("UNKNOWN MODULE.".into())).await.ok();
            tx.send(OutputLine::Done).await.ok();
            Ok(())
        }
    }
}
