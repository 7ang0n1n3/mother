# MU/TH/UR 6000

**Network Reconnaissance Suite** — a terminal UI built with [Ratatui](https://github.com/ratatui-org/ratatui), styled after the shipboard computer from *Alien*.

```
SYSTEM INITIALIZATION COMPLETE.
MU/TH/UR 6000 ONLINE.
WEYLAND-YUTANI CORP — NETWORK RECONNAISSANCE SUITE.
7 MODULES LOADED. ALL SYSTEMS NOMINAL.
```

## Features

| Module | Description | Target syntax |
|---|---|---|
| **PORT SCAN** | Async TCP connect scan (500 concurrent) | `HOST [PORT_SPEC]` — e.g. `192.168.1.1 1-1024`, `host.com 22,80,443` |
| **PING** | ICMP echo probe | `HOST [COUNT]` — e.g. `8.8.8.8 5` |
| **TRACEROUTE** | Network path analysis | `HOST` — e.g. `8.8.8.8` |
| **DNS LOOKUP** | Resource record query | `DOMAIN [TYPE]` — e.g. `example.com MX` · types: A AAAA MX NS TXT CNAME SOA PTR SRV CAA |
| **ARP SCAN** | Local network host discovery | `NETWORK/CIDR` — e.g. `192.168.1.0/24` · blank = auto-detect subnet |
| **WHOIS** | Domain / IP registration lookup | `DOMAIN or IP` — e.g. `example.com`, `1.1.1.1` |
| **MTR** | Continuous route analysis with loss/latency stats | `HOST [CYCLES]` — e.g. `8.8.8.8 10` |

## Requirements

Runs fully in userspace — no root or `sudo` required.

| Module | Needs |
|---|---|
| Port scan, DNS, Whois | Nothing extra |
| Ping | Linux: `net.ipv4.ping_group_range` must include your GID (default on most distros) |
| ARP Scan | Nothing extra (uses TCP probe sweep + kernel ARP cache) |
| Traceroute | `traceroute` (Linux/macOS) or built-in `tracert` (Windows) |
| MTR | `mtr` if available; falls back to `traceroute`/`tracert` automatically |

### Install optional tools

```bash
# Arch / CachyOS
sudo pacman -S traceroute mtr

# Debian / Ubuntu
sudo apt install traceroute mtr-tiny

# macOS (Homebrew)
brew install mtr
```

## Build & Run

```bash
cargo build --release
./target/release/mother
```

## Keybindings

| Key | Action |
|---|---|
| `↑` / `k` | Select previous module |
| `↓` / `j` | Select next module |
| `Enter` / `Tab` | Enter input mode for selected module |
| `Esc` / `Tab` | Cancel input, return to browse |
| `Enter` | Execute scan |
| `Ctrl+U` | Clear input |
| `PgUp` / `PgDn` | Scroll output |
| `M` | Toggle audio mute |
| `Q` / `Ctrl+C` | Quit |

## Notes

- **Whois**: queries IANA for an authoritative referral first, then falls back through a built-in server list per TLD. If all servers fail, port 43 is likely blocked by your network — this is common on corporate/ISP networks.
- **ARP Scan**: does not send raw Ethernet frames. Instead it probes hosts via TCP to trigger kernel ARP resolution, then reads `/proc/net/arp`. Results reflect hosts that responded; passive ARP cache entries are also shown.
- **MTR**: when `mtr` is not installed, automatically falls back to a single `traceroute`/`tracert` pass — no statistics columns, but the hop path is still shown.
