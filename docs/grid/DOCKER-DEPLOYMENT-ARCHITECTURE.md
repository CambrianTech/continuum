# Deployment Architecture — DRAFT v2

**Status:** DRAFT — needs review by Joel + M5 Claude before implementation.

## The Problem

The tray app, CLI, and setup.sh all need to answer the same questions:
1. Is Continuum running? How?
2. What URL opens the UI?
3. What nodes are on the grid?
4. What actions are available?

Currently each tool answers these differently, with hardcoded ports, broken assumptions, and no awareness of mode. The result: tray shows "CLI unavailable" while 5 personas are chatting.

## The Runtime Matrix

Two independent axes:

**Server mode** (how Continuum runs):
| Mode | HTTP Port | WS Port | How it starts | Who serves UI |
|------|-----------|---------|---------------|---------------|
| Docker | 9003 | 9001 | `docker compose up -d` | widget-server container |
| Bare metal | from config.env (default 9000) | from config.env (default 9001) | `npm start` in src/ | node orchestrator (TypeScript) |
| None | — | — | — | — |

**Network mode** (how it's reachable):
| Mode | How to detect | Local URL | Remote URL |
|------|---------------|-----------|------------|
| Tailscale Serve | `tailscale serve status` has entries | `https://<dns-name>` | `https://<dns-name>` |
| Tailscale | `tailscale ip -4` succeeds | `http://<ts-ip>:<http-port>` | `http://<ts-ip>:<http-port>` |
| None | no tailscale | `http://localhost:<http-port>` | unreachable |

## Five Functions (the whole API)

Everything the tray, CLI, and setup.sh need comes from five functions. Each has one job.

### 1. `detect_server() → {mode, http_port, ws_port}`

```
Is Docker running AND has healthy Continuum containers?
  → {mode: "docker", http_port: 9003, ws_port: 9001}

Is bare metal running? (check: can we connect WS to config.env WS_PORT?)
  → {mode: "bare-metal", http_port: $HTTP_PORT, ws_port: $WS_PORT}

Neither?
  → {mode: "none", http_port: 0, ws_port: 0}
```

Detection order matters: Docker first (it's the production path), then bare metal.

Bare metal detection: `curl -sf http://localhost:$HTTP_PORT` or check if orchestrator PID exists.
Docker detection: `docker compose ps --format '{{.Health}}' | grep -c healthy`

### 2. `detect_network() → {mode, ts_ip, dns_name, tailnet}`

```
Is Tailscale Serve active? (`tailscale serve status` has proxy entries)
  → {mode: "serve", ts_ip: "...", dns_name: "<name>.<tailnet>", tailnet: "..."}

Is Tailscale connected? (`tailscale ip -4` succeeds)
  → {mode: "tailscale", ts_ip: "...", dns_name: "", tailnet: "..."}

No Tailscale?
  → {mode: "local", ts_ip: "", dns_name: "", tailnet: ""}
```

### 3. `get_url(server, network) → string`

```
network.mode == "serve"     → "https://{network.dns_name}"
network.mode == "tailscale" → "http://{network.ts_ip}:{server.http_port}"
network.mode == "local"     → "http://localhost:{server.http_port}"
server.mode == "none"       → "" (no URL)
```

Three lines. No if-else chains. No hardcoded ports.

### 4. `get_nodes(network) → [{name, ip, online, url, is_self, has_ui}]`

```
If network.tailnet is empty → return []

For each peer in `tailscale status`:
  is_self = (peer.ip == network.ts_ip)
  online = (peer.status != "offline")
  
  url:
    is_self               → get_url(server, network)  # reuse function 3
    peer has "-grid"      → "https://{peer.name}.{network.tailnet}"
    peer is online        → "http://{peer.ip}:9003"   # assume Docker default
    peer is offline       → null
  
  has_ui:
    url != null → curl -sf $url with 2s timeout → true/false
```

### 5. `get_actions(server, network) → [{id, label, command}]`

```
server.mode == "none":
  → [{start-docker, "Start Docker Desktop", "open -a Docker"}]
  → [{doctor, "Doctor", "continuum doctor"}]

server.mode != "none" AND healthy == 0:
  → [{start, "Start Services", "continuum start"}]
  → [{doctor, "Doctor", "continuum doctor"}]

server.mode != "none" AND healthy > 0:
  → [{open, "Open Browser", "continuum open"}]
  → [{restart, "Restart", "continuum restart"}]
  → [{stop, "Stop", "continuum stop"}]
  → [{update, "Update", "continuum update"}]
  → [{logs, "Logs", "continuum logs"}]
```

## Platform-Specific Concerns

### macOS
- Docker: Docker Desktop or Rancher Desktop (VM-based)
- Tray: Swift .app bundle in ~/Applications/Continuum.app
- Auto-start: LaunchAgent plist
- Ports: Docker maps 9003 (HTTP) and 9001 (WS) to host

### Windows (WSL2)
- Docker: Docker Desktop on Windows, accessed from WSL via docker.exe wrapper
- Docker socket: NOT at /var/run/docker.sock — needs wrapper script
- Tray: PowerShell NotifyIcon, calls `wsl.exe --exec continuum tray-data`
- Auto-start: .bat in Windows Startup folder with `-ExecutionPolicy Bypass`
- Ports: Same as macOS but through WSL2 network layer
- Tailscale: Runs on BOTH Windows host AND WSL2 (different instances)
- `/dev/net/tun`: NOT available in WSL2 — Tailscale container can't run, use host Tailscale

### Linux
- Docker: Native daemon, no VM
- Tray: Python GTK AppIndicator
- Auto-start: systemd user service
- Ports: Direct, no VM layer
- Tailscale: Native, including Tailscale container for grid mode

## Port Configuration (NEVER hardcoded)

Source of truth: `~/.continuum/config.env`
```
HTTP_PORT=9000    # bare metal HTTP server
WS_PORT=9001     # WebSocket server (both modes)
```

Docker overrides via environment in docker-compose.yml:
```
JTAG_HTTP_PORT=9003         # widget-server listens here
JTAG_WEBSOCKET_PORT=9001    # injected into browser config
NODE_WS_PORT=9001           # host port mapping for WS
```

The five functions read these values. Nothing else touches ports directly.

## jtag Over the Grid

Local: `./jtag ping` → WS to localhost:$WS_PORT
Remote: `continuum @bigmama ping` → WS to localhost:$WS_PORT → grid/send → TCP to bigmama → executes `ping` → returns result

The `@node` prefix triggers grid routing. Without it, everything is local. The grid transport (TCP over Tailscale) is transparent.

## What's Broken Today

1. **Tray can't find CLI** — app bundle doesn't load PATH. Fix: Swift code uses `cliBin` lazy var that searches known paths. Needs to include `~/.local/bin/continuum`.
2. **Tray shows "CLI unavailable"** — the `runCLI` method uses `/bin/bash -c` which doesn't load profile. Fix: use `/bin/bash -l -c` (login shell).
3. **tray-data only counts Docker containers** — doesn't detect bare metal. Fix: `detect_server()` checks both.
4. **URLs hardcoded** — "localhost:9003" baked in everywhere. Fix: `get_url()` reads ports from config.
5. **Node URLs assume HTTPS** — only grid nodes have TLS. Fix: `get_nodes()` checks per-node.
6. **WSL2 Docker socket missing** — setup.sh creates wrapper but guard condition was wrong. Fixed: checks for actual file, uses full docker.exe path.
7. **Tailscale container fails on WSL2** — no /dev/net/tun. Fix: detect WSL2, skip Tailscale container, use host Tailscale.

## Implementation Order

1. Write `detect_server()` and `detect_network()` in the `continuum` CLI (bash)
2. Rewrite `cmd_tray_data` using the five functions
3. Test on all three machines (M1 Pro, M5, BigMama)
4. Update Swift tray to use login shell + verify CLI path
5. Update PowerShell tray to use `wsl.exe --exec` with full path
6. Update `setup.sh` to use same functions for URL display
7. Add `@node` routing to `continuum` CLI for remote jtag

## Open Questions

- Should the tray auto-recover (start Docker/containers) or just show the state?
- How does the tray detect bare metal health without jtag? (jtag needs WS which takes 2s)
- Should `continuum tray-data` cache results to avoid 30s Tailscale CLI calls?
- Windows: should we ship a native .exe tray instead of PowerShell?
