# WSL2 + Docker Guide for continuum

Companion to [`WINDOWS-WSL2-INSTALL-GUIDE.md`](./WINDOWS-WSL2-INSTALL-GUIDE.md). That doc covers the initial Windows→WSL2 onboarding. This one covers the **Docker**-specific questions that come up once you're in WSL2: Docker Desktop integration toggles, the Git-Bash vs WSL decision, and the network gotchas that eat the most time.

---

## Docker Desktop WSL Integration

Docker Desktop runs the Docker daemon inside its own lightweight VM and *shares* it with WSL2 distros on demand. By default only the default distro (usually `Ubuntu`) gets Docker access. If your distro doesn't, every `docker ps` inside WSL returns `Cannot connect to the Docker daemon at unix:///var/run/docker.sock`.

### Enable integration manually (GUI)

1. Launch Docker Desktop.
2. Settings → Resources → WSL Integration.
3. Toggle "Enable integration with my default WSL distro" *and* toggle your specific distro in the list below.
4. Click "Apply & Restart".
5. Back in WSL: `docker version` should now show both Client and Server.

### Enable via `settings-store.json` (scriptable)

Docker Desktop writes its settings to `%APPDATA%\Docker\settings-store.json` on Windows, readable as `/mnt/c/Users/<YOU>/AppData/Roaming/Docker/settings-store.json` from WSL. The relevant keys:

```json
{
  "IntegratedWslDistros": ["Ubuntu-24.04"],
  "EnableIntegrationWithDefaultWslDistro": true
}
```

continuum's `install-common.sh` ships a `mod_wsl_integration` module that edits this file automatically and tells the user to click "Apply & Restart" if Docker Desktop is already running.

### Verify

```bash
docker version                        # must show Server: Docker Desktop
docker info | grep -i 'operating system'   # Docker Desktop
docker run --rm hello-world          # full round-trip through the shared daemon
```

If `docker version` hangs for 30s then fails, Docker Desktop isn't running on Windows. Start it first, then try again.

---

## Git Bash vs WSL: which should I use?

**Short answer: WSL2 for continuum, always.** Git Bash is a stripped-down MSYS2 shell that looks POSIX-y but isn't. It breaks continuum in three specific ways:

| Concern | Git Bash | WSL2 |
|---------|----------|------|
| `bash` scripts in `src/scripts/` | mostly run, but path quoting is fragile | run exactly as on Linux |
| Unix domain sockets | emulated; `~/.continuum/sockets/*.sock` paths get mangled through `/c/Users/...` translation | real POSIX sockets, paths clean |
| `cargo build` for Rust workers | requires MSVC or MinGW toolchain, hits the issues documented in the onboarding guide | standard Linux toolchain works |
| `docker exec` into running containers | works, but TTY detection is unreliable | works cleanly |
| Line endings in generated files | CRLF sneaks in via core.autocrlf defaults | LF everywhere |

If you're on Windows, do installs and all day-to-day driving from WSL2. Use Git Bash only for `git` operations that specifically need to hit Windows-side paths (rare in this project).

### When Git Bash is fine

- Cloning the repo into `C:\...` for a Windows-side editor to see.
- One-off `gh` CLI commands that don't touch the runtime.
- Running `wsl --install` or other Windows-host commands.

That's it. All `npm start`, `install.sh`, `cargo`, `docker compose`, `continuum doctor` should run inside WSL2.

---

## WSL2 + Docker Networking Gotchas

WSL2 has three network faces, and which one your container sees depends on config:

1. **`localhost` loopback** — WSL2 ↔ Windows host ↔ Docker Desktop are all linked via a Hyper-V virtual network. `localhost:PORT` from inside WSL typically reaches a port a Windows process is listening on, AND a port a Docker Desktop container is publishing. Usually this Just Works; occasionally it races.
2. **The WSL2 VM's own IP** — `hostname -I` in WSL gives you the VM-local IP (e.g. `172.29.x.x`). Useful for reaching Windows services from inside a container (`host.docker.internal` resolves to the Windows host, not the WSL VM, which matters when the service you want is running *in* WSL).
3. **Tailscale on the Windows host vs. the WSL VM** — Tailscale installed on Windows exposes a tailnet IP reachable from WSL automatically. Tailscale installed inside WSL gets a *different* tailnet IP. If continuum peers pair via `airc connect` over tailscale, agree on which side runs tailscale before pairing — mixing both is a debugging nightmare.

### Specific gotchas

**`localhost` from inside a container doesn't mean the host.**
Inside a container, `localhost` = the container itself. To reach a Windows process (e.g. an SSH agent, Tailscale daemon, Docker Desktop), use `host.docker.internal`. To reach a service running in WSL2 (not inside a container, but on the WSL VM), use the WSL VM's IP — `host.docker.internal` does NOT resolve to WSL. Find it with:
```bash
ip route show | awk '/default/ {print $3}'   # gateway = Windows host
hostname -I | awk '{print $1}'               # WSL VM IP
```

**WSL2 mirrored networking mode** (Windows 11 22H2+) changes all of the above.
When `networkingMode=mirrored` is set in `%USERPROFILE%\.wslconfig`, WSL uses the Windows network stack directly. `hostname -I` returns the Windows IP, and `localhost` is symmetric. If you're on mirrored mode, expect `host.docker.internal` to sometimes be unnecessary. Check with:
```bash
cat /mnt/c/Users/$WIN_USER/.wslconfig 2>/dev/null | grep -i networkingMode
```

**Port forwarding works both ways but at different costs.**
Docker Desktop auto-forwards published container ports to `localhost` on Windows. WSL2's NAT mode auto-forwards Windows-host-bound ports into WSL. Mirrored mode skips the forwarding (same stack). If you see "connection refused" on what should be a reachable port, try:
```bash
ss -tlnp | grep PORT      # in WSL, is anything listening?
netsh interface portproxy show all   # on Windows, are there proxies set up?
```

**GPU passthrough requires WSL kernel + NVIDIA driver ≥ 510 on Windows.**
Check with `nvidia-smi` from inside WSL. If it fails, the fix is updating the Windows NVIDIA driver, not the Linux one — WSL uses the Windows driver via shim. After driver update, run `wsl --shutdown` then restart WSL.

**continuum IPC sockets live at `~/.continuum/sockets/*.sock` in WSL's $HOME.**
From inside containers: mount that path in with `-v ~/.continuum:/root/.continuum` (the repo's compose files already do this). From Windows natively: the path is `\\wsl.localhost\Ubuntu-24.04\home\<user>\.continuum\sockets\*`, but Windows apps can't dial unix sockets directly even when they can see the file — use WSL2.

---

## Quick diagnostic

```bash
# Run this inside WSL2 after an install to sanity-check the Docker + network path.
echo "--- wsl distro ---"; cat /etc/os-release | grep -E "^(NAME|VERSION)="
echo "--- docker ---";    docker version --format '{{.Server.Version}} @ {{.Server.Os}}/{{.Server.Arch}}'
echo "--- gpu ---";       nvidia-smi -L 2>/dev/null || echo "no GPU"
echo "--- networking ---"
echo "  WSL IP:       $(hostname -I | awk '{print $1}')"
echo "  Windows host: $(ip route show | awk '/default/ {print $3}')"
echo "  Mirrored:     $(grep -q networkingMode=mirrored /mnt/c/Users/*/.wslconfig 2>/dev/null && echo yes || echo no)"
echo "--- continuum IPC ---"; ls -la ~/.continuum/sockets/ 2>/dev/null || echo "no sockets yet"
```

If any of those come back surprising, that's likely the symptom the user will report.
