# Windows Install Guide (GPU)

## Prerequisites

- Windows 10/11 with WSL2 enabled
- NVIDIA GPU with drivers installed (Game Ready or Studio)

## Step 1: Docker Desktop

1. Download from https://docs.docker.com/desktop/setup/install/windows-install/
2. Run installer — select **"Use WSL 2 instead of Hyper-V (recommended)"** (this is the default)
3. Skip Docker Hub sign-in (not required)
4. After install, open Docker Desktop and configure:
   - **Settings → General → "Start Docker Desktop when you sign in"** → ENABLE
   - **Settings → Resources → WSL Integration → Ubuntu** → ENABLE
   - Apply & Restart
5. Restart Windows when prompted

### Why WSL2 not Hyper-V?

WSL2 passes NVIDIA drivers straight through — CUDA just works. Hyper-V requires GPU-P (partitioning) which is complicated and poorly supported for CUDA.

## Step 2: Tailscale (optional — only for grid/remote access)

Skip this if running standalone. Only needed if you want to reach this machine from other devices on the mesh.

1. Download from https://tailscale.com/download/windows
2. Run installer, sign in with your account
3. Tailscale runs as a Windows service — survives reboots automatically

**Why Windows app, not inside WSL2 or Docker?** If Docker crashes, you can still SSH in to fix it. Tailscale in a container means Docker down = locked out.

### Enable HTTPS for voice/video (requires Tailscale Starter plan — $6/month)

Voice and video calls require TLS. Tailscale provides free valid HTTPS certs for your devices.

1. Upgrade to Tailscale Starter plan at https://login.tailscale.com/admin/billing
2. Go to https://login.tailscale.com/admin/dns
3. Enable **HTTPS Certificates** (toggle at bottom of page)
4. MagicDNS should already be enabled (default)
5. Provision certs on each machine:
   ```bash
   tailscale cert <hostname>.<tailnet>.ts.net
   # Example: tailscale cert bigmama.taila5cb68.ts.net
   ```
6. Move certs to `~/.continuum/`:
   ```bash
   mv *.ts.net.crt *.ts.net.key ~/.continuum/
   ```

Continuum auto-detects these certs and serves HTTPS. No config needed.

**Without Tailscale paid:** Everything works except voice/video. Browsers block microphone/camera access on non-HTTPS connections.

## Step 3: Verify GPU passthrough

No extra CUDA or toolkit install needed — Docker Desktop handles GPU passthrough automatically via WSL2. Just verify it works:

```bash
docker run --rm --gpus all nvidia/cuda:12.8.0-base-ubuntu24.04 nvidia-smi
```

You should see your GPU listed. If not, restart Docker Desktop.

## Step 4: Run Continuum

```bash
# Clone (if not already)
git clone https://github.com/CambrianTech/continuum.git
cd continuum

# Pull pre-built images and start
docker compose up

# With GPU forging:
docker compose --profile gpu up
```

### Or install via script

```bash
curl -fsSL https://raw.githubusercontent.com/CambrianTech/sentinel-ai/main/install.sh | bash
```

## Boot Order (automatic after setup)

1. **Windows boots** → Tailscale service starts (remote access works)
2. **Docker Desktop auto-starts** → WSL2 integration activates
3. **Containers with `restart: unless-stopped`** → come up automatically

No manual intervention needed after reboot.

## Troubleshooting

### `docker` command not found in WSL2
Docker Desktop → Settings → Resources → WSL Integration → enable your distro

### GPU not visible in containers
1. Verify drivers: `nvidia-smi` (in WSL2, outside Docker)
2. Verify toolkit: `docker run --rm --gpus all nvidia/cuda:12.8.0-base-ubuntu24.04 nvidia-smi`
3. Restart Docker Desktop

### Can't SSH after reboot
Tailscale Windows app not running. Check system tray. If installed inside WSL2 instead, migrate to Windows app.

### Containers don't start after reboot
Docker Desktop → Settings → General → "Start Docker Desktop when you sign in" must be ON
