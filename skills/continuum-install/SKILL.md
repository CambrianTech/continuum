---
name: continuum:install
description: Walk a new user through first-time Continuum installation. Detects platform, verifies prerequisites, runs install.sh, validates first-chat readiness.
user-invocable: true
allowed-tools: Bash, Read
argument-hint: "[--carl | --dev]"
---

# Install Continuum

Drive the installation yourself. The user invoked this because they want Continuum running — don't explain what it is, just get them there.

## 1. Detect platform

```bash
OS=$(uname -s)
ARCH=$(uname -m)
echo "Platform: $OS $ARCH"
```

## 2. Verify prerequisites

### All platforms
- **Docker Desktop** (Mac/Windows) or **Docker Engine** (Linux): `docker info >/dev/null 2>&1`
- **Git**: `command -v git`

### Mac-specific
- Docker Desktop 4.62+ (for Model Runner): `docker model --help 2>/dev/null`
- If missing: tell user to install Docker Desktop from docker.com, launch it, then re-run

### Windows/WSL2
- Verify WSL2: `uname -r | grep -i microsoft`
- Docker Desktop with WSL2 integration: `docker info`

### Linux
- NVIDIA GPU (optional): `nvidia-smi 2>/dev/null`

## 3. Run install.sh

**Carl path (default — prebuilt images, fast):**
```bash
curl -fsSL https://cambriantech.github.io/continuum/install.sh | bash
```

**Dev path (clone + build from source):**
```bash
git clone https://github.com/CambrianTech/continuum.git ~/continuum
cd ~/continuum && bash install.sh
```

Stream the output to the user. Don't silence it — they should see progress.

## 4. Validate first-chat readiness

After install completes:

```bash
# Verify Docker Model Runner has the default model
docker model ls 2>/dev/null | grep -i qwen

# Verify the stack is healthy
cd ~/continuum/src && ./jtag ping 2>/dev/null | grep -q systemReady && echo "✅ System ready" || echo "⚠️ System not ready yet — wait 30s and try: ./jtag ping"
```

## 5. First chat test

```bash
cd ~/continuum/src && ./jtag collaboration/chat/send --room="general" --message="Hello from a fresh install! Any persona please respond."
```

Wait 15-30 seconds, then:
```bash
./jtag collaboration/chat/export --room="general" --limit=5
```

If a persona replied → installation successful. If not → run `/continuum:doctor`.

## Mac keychain note

macOS may prompt for your login keychain password during model download (Docker accessing cached HuggingFace credentials). Enter your Mac login password and click Allow.

## Failure modes

- **Docker not installed**: print install URL + platform-specific instructions, stop
- **Docker not running**: tell user to launch Docker Desktop, wait for whale icon, re-run
- **Model pull fails**: check network, retry `docker model pull hf.co/continuum-ai/qwen3.5-4b-code-forged-GGUF`
- **No personas reply**: run `/continuum:doctor` for diagnosis
