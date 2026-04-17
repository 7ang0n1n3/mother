# Changelog

All notable changes to MU/TH/UR 6000 are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

---

## [0.2.0] — 2026-03-13

### Changed — userspace operation (no root required)

**Traceroute**
- Replaced hand-rolled raw `ICMP4` socket (required `CAP_NET_RAW`) with a
  subprocess call to the system `traceroute` tool.
- On Windows the command is automatically switched to `tracert -d`.
- Header and summary lines from the tool are filtered; hops are colour-coded
  (dim for timeouts, bright for the destination).

**MTR**
- Replaced hand-rolled raw `ICMP4` socket with a subprocess call to `mtr
  --report --no-dns`.
- If `mtr` is not installed, automatically falls back to `traceroute`/`tracert`
  and emits a single path trace with a notice that statistics are unavailable.

**ARP Scan**
- Replaced `pnet` raw Ethernet datalink channel (required `CAP_NET_RAW`) with a
  userspace approach: concurrent TCP probes to all subnet hosts trigger kernel
  ARP resolution; results are read from `/proc/net/arp`.
- Pre-existing ARP cache entries are merged with newly resolved entries.
- Subnet detection for the blank-target case still uses `pnet::datalink` for
  interface enumeration (read-only, no raw socket needed).

**Whois**
- Added an ordered per-TLD fallback server list so that IANA failure no longer
  silently falls back to a single heuristic server.
  - `.com`/`.net`/`.edu` → `whois.verisign-grs.com` → `whois.internic.net`
  - IP addresses → ARIN → RIPE → APNIC → LACNIC → AFRINIC
  - Additional TLDs: `.org`, `.io`, `.uk`, `.de`, `.fr`, `.nl`, `.au`, `.ca`,
    `.jp`, `.cn`, `.br`, `.in`
- Connection errors (OS 101/111/113, timeout) now move to the next server
  rather than aborting immediately.
- Terminal error message identifies port 43 filtering as the likely cause when
  all servers fail with connection errors.
- IANA is now a referral hint only; its failure is silent and the candidate
  list takes over immediately.

### Fixed

- `traceroute` and `mtr` output header lines (e.g. `tracing route to …`,
  `trace complete`) are now suppressed regardless of platform.
- Whois `IANA UNREACHABLE` message no longer appears; IANA failure is handled
  silently with graceful fallback.

---

## [0.1.0] — 2026-03-13

### Added

- **PORT SCAN** — async TCP connect scan with up to 500 concurrent connections;
  supports single port, comma list, and `lo-hi` range syntax; common service
  names annotated in output.
- **PING** — ICMP echo probe via `surge-ping`; supports custom packet count;
  reports per-packet RTT and summary statistics.
- **TRACEROUTE** — hop-by-hop network path analysis using raw ICMP with TTL
  manipulation.
- **DNS LOOKUP** — resource record queries via `hickory-resolver`; supports A,
  AAAA, MX, NS, TXT, CNAME, SOA, PTR, SRV, CAA.
- **ARP SCAN** — raw Ethernet ARP request/reply sweep via `pnet`; auto-detects
  local subnet; refuses subnets larger than /16.
- **WHOIS** — two-stage lookup: IANA referral then authoritative registrar;
  heuristic TLD-to-server mapping as fallback.
- **MTR** — continuous multi-cycle route analysis; reports loss%, last, avg,
  best, worst, and standard deviation per hop.
- Ratatui TUI with three interaction modes: Browse, Input, Running.
- Amber-on-black retro terminal aesthetic with Weyland-Yutani / MU/TH/UR 6000
  theming.
- Ambient sound engine: boot chime, keypress clicks, scan tones, error beeps,
  completion sound; toggle with `M`.
- Output auto-scroll with manual `PgUp`/`PgDn` override.
