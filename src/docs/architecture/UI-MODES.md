# UI Modes — Presentation Layer Architecture

Continuum supports four UI modes. All hit the same command layer (`Commands.execute()`). The mode determines which presentation is active, not what's possible.

## Modes

### 1. Headless (No UI)
- **Use case:** AWS, RunPod, background GPU workers, compute-only towers
- **Interface:** None — commands received over the wire from other towers
- **Startup:** `npm start --headless` (no browser launch, no TUI)
- **Health check:** HTTP ping only, no browser wait

### 2. CLI (Cyberpunk TUI)
- **Use case:** Terminal-first users, SSH sessions, developers, headless machines with a terminal
- **Interface:** Interactive themed terminal (Rich-based, mouse + keyboard)
- **Features:** Chat with AIs, monitor training, manage models, tower status, theme switching
- **Framework:** [cyberpunk-cli](https://github.com/CambrianTech/cyberpunk-cli) — Loki/Matrix/Fallout/Tron themes
- **Startup:** `jtag --interactive` or `jtag tui`

### 3. Web (Browser UI)
- **Use case:** Desktop users, full visual experience
- **Interface:** Browser with avatars, live voice, visual workspace
- **Features:** Everything — avatars, WebRTC voice, chat, live rooms, settings
- **Startup:** `npm start` (default — launches browser)

### 4. CLI + Web (Both)
- **Use case:** Power users — TUI for quick commands while browser handles visual
- **Interface:** Both simultaneously
- **Startup:** `npm start` + `jtag tui` in another terminal

## Architecture

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│   Web (Browser)  │  │  CLI (TUI)   │  │  Headless    │
│   HTML/JS/CSS│  │  Rich/Python │  │  (no UI)     │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │
       └────────┬────────┴────────┬────────┘
                │                 │
         ┌──────┴──────┐   ┌─────┴──────┐
         │  WebSocket  │   │  Unix/TCP  │
         │  Transport  │   │  Transport │
         └──────┬──────┘   └─────┬──────┘
                │                │
         ┌──────┴────────────────┴──────┐
         │     Commands.execute()       │
         │     (Unified Command Layer)  │
         └──────────────┬───────────────┘
                        │
         ┌──────────────┴───────────────┐
         │    Tower Router              │
         │    (local or remote)         │
         └──────────────────────────────┘
```

## Detection / Selection

The active mode is determined by:
- `--headless` flag or `CONTINUUM_HEADLESS=1` → headless
- `jtag tui` or `jtag --interactive` → CLI mode
- Default `npm start` → web mode (launches browser)
- No display detected (`$DISPLAY` empty on Linux, no WindowServer on macOS) → auto-headless

## Implementation Status

| Mode | Status | Notes |
|------|--------|-------|
| Web | ✅ Production | Full browser UI |
| Headless | 🚧 Partial | Server runs, health check needs `--headless` flag |
| CLI | 📋 Planned | cyberpunk-cli framework ready, needs jtag integration |
| CLI + Web | 📋 Planned | Both transports already work, needs TUI client |
