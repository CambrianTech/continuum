# Configuration Guide

Continuum uses a single configuration file for all settings: `~/.continuum/config.env`

## Quick Start

```bash
# Config is auto-created on first run
npm start

# Or create manually
open ~/.continuum/config.env
```

## Configuration File

Location: `~/.continuum/config.env`

The file is auto-created with documented defaults on first run. It is **never overwritten** — your settings are preserved across updates.

---

## AI Provider API Keys

Continuum integrates with multiple AI providers. Add your API keys to enable each provider.

### Supported Providers

| Provider | Key | Capabilities | Cost |
|----------|-----|--------------|------|
| **Anthropic** | `ANTHROPIC_API_KEY` | Claude models (Opus, Sonnet, Haiku) | Pay per token |
| **OpenAI** | `OPENAI_API_KEY` | GPT-4, GPT-3.5, fine-tuning, embeddings | Pay per token |
| **X.AI** | `XAI_API_KEY` | Grok models | Pay per token |
| **DeepSeek** | `DEEPSEEK_API_KEY` | DeepSeek models, fine-tuning | Pay per token |
| **Together AI** | `TOGETHER_API_KEY` | Open models, fine-tuning, fast inference | Pay per token |
| **Fireworks AI** | `FIREWORKS_API_KEY` | Fast inference, fine-tuning | Pay per token |
| **Groq** | `GROQ_API_KEY` | Ultra-fast inference (LPU) | Pay per token |
| **Mistral AI** | `MISTRAL_API_KEY` | Mistral models | Pay per token |
| **HuggingFace** | `HUGGINGFACE_API_KEY` | Model hub, inference API | Free tier + paid |
| **Ollama** | (local) | Local models, free, private | Free (your hardware) |

### Example Configuration

```env
# Required for AI responses (at least one)
ANTHROPIC_API_KEY=sk-ant-api03-xxxxx
OPENAI_API_KEY=sk-xxxxx

# Optional - additional providers
XAI_API_KEY=xai-xxxxx
DEEPSEEK_API_KEY=sk-xxxxx
TOGETHER_API_KEY=xxxxx
FIREWORKS_API_KEY=xxxxx
FIREWORKS_ACCOUNT_ID=xxxxx
GROQ_API_KEY=gsk_xxxxx
MISTRAL_API_KEY=xxxxx
HUGGINGFACE_API_KEY=hf_xxxxx
```

### Provider Capabilities

```
┌─────────────────────────────────────────────────────────────────────┐
│                    PROVIDER CAPABILITIES                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Provider        Chat    Fine-Tune    Embeddings    Speed          │
│   ─────────────────────────────────────────────────────────────     │
│   Anthropic       ✅       ✅           ❌           Fast            │
│   OpenAI          ✅       ✅           ✅           Fast            │
│   Together        ✅       ✅           ✅           Fast            │
│   Fireworks       ✅       ✅           ✅           Very Fast       │
│   Groq            ✅       ❌           ❌           Ultra Fast      │
│   DeepSeek        ✅       ✅           ✅           Fast            │
│   Ollama          ✅       ✅*          ✅           Local           │
│                                                                      │
│   * Ollama fine-tuning requires local GPU and Unsloth               │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Ollama (Local Inference)

Ollama runs locally — no API key needed, completely private.

```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3.2:3b

# Continuum auto-detects running Ollama
npm start
```

Ollama models are used for local PersonaUsers (Helper AI, Teacher AI, etc.).

---

## Networking

```env
# HTTP server port (default: 9000)
HTTP_PORT=9000

# WebSocket server port (default: 9001)
WS_PORT=9001
```

Open `http://localhost:9000` to access the UI.

---

## Logging Configuration

```env
# Log level: debug, info, warn, error, silent
LOG_LEVEL=info

# Add timestamps to logs (0 or 1)
LOG_TIMESTAMPS=1

# Output to console (0 or 1)
LOG_TO_CONSOLE=1

# Write to log files (0 or 1)
LOG_TO_FILES=1

# File mode: clean (fresh each session), append, archive
LOG_FILE_MODE=clean
```

### Log Locations

```
~/.continuum/
├── sessions/user/shared/*/logs/
│   ├── server.log        # Server-side logs
│   └── browser.log       # Browser console logs
└── personas/
    ├── helper/logs/      # Per-persona logs
    ├── teacher/logs/
    └── .../
```

---

## Database Configuration

```env
# Override default database locations (optional)
# DATABASE_DIR=~/.continuum/data
# DATABASE_BACKUP_DIR=~/.continuum/data/backups
# DATABASE_ARCHIVE_DIR=~/.continuum/data/archive

# Datasets directory for training
# DATASETS_DIR=/path/to/datasets
```

Default: All data in `~/.continuum/data/`

---

## Feature Flags

```env
# Use Rust-based AI provider (experimental)
USE_RUST_AI_PROVIDER=0

# Enable automatic data archiving
ENABLE_ARCHIVE_DAEMON=0
```

---

## Loading Priority

Secrets are loaded in this order (later sources override earlier):

1. `~/.continuum/config.env` — Primary configuration
2. `process.env` — System environment variables
3. `.env` — Project-local (for development)

---

## Security Best Practices

### What's Protected

- **Server-side only**: API keys never sent to browser
- **Automatic redaction**: Keys filtered from logs and screenshots
- **Audit trail**: Access logging for security review
- **Graceful degradation**: Missing keys don't crash system

### Don't Commit Secrets

```gitignore
# Already in .gitignore
.env
config.env
~/.continuum/config.env
```

### Verify Configuration

```bash
# Check which providers are configured
./jtag ai/report

# Test a specific provider
./jtag ping
```

---

## Complete Template

This is auto-generated at `~/.continuum/config.env` on first run:

```env
# ============================================
# CONTINUUM CONFIGURATION
# ============================================

# ============================================
# NETWORKING
# ============================================
HTTP_PORT=9000
WS_PORT=9001

# ============================================
# LOGGER CONFIGURATION
# ============================================
LOG_LEVEL=info
LOG_TIMESTAMPS=1
LOG_TO_CONSOLE=1
LOG_TO_FILES=1
LOG_FILE_MODE=clean

# ============================================
# API KEYS
# ============================================
ANTHROPIC_API_KEY=
OPENAI_API_KEY=
XAI_API_KEY=
DEEPSEEK_API_KEY=
TOGETHER_API_KEY=
FIREWORKS_API_KEY=
FIREWORKS_ACCOUNT_ID=
GROQ_API_KEY=
MISTRAL_API_KEY=
HUGGINGFACE_API_KEY=

# ============================================
# DATABASE PATHS (optional)
# ============================================
#DATABASE_DIR=~/.continuum/data
#DATABASE_BACKUP_DIR=~/.continuum/data/backups
#DATABASE_ARCHIVE_DIR=~/.continuum/data/archive
#DATASETS_DIR=/path/to/datasets

# ============================================
# FEATURE FLAGS
# ============================================
USE_RUST_AI_PROVIDER=0
ENABLE_ARCHIVE_DAEMON=0
```

---

## See Also

- [GETTING-STARTED.md](../GETTING-STARTED.md) — Full setup walkthrough
- [CLAUDE.md](../CLAUDE.md) — Development guide
