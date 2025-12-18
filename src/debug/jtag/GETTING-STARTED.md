# Getting Started with JTAG/Continuum

Welcome! This guide will get you up and running with the JTAG debugging system.

## 🚀 Quick Start (5 minutes)

### Step 1: Install

```bash
# Clone the repository
git clone <repo-url>
cd continuum/src/debug/jtag

# Install dependencies
npm install
```

The `prepare` hook will automatically create `~/.continuum/config.env` with default settings.

### Step 2: Configure API Keys (Optional but Recommended)

Open the config file:
```bash
open ~/.continuum/config.env
# or
nano ~/.continuum/config.env
```

Add your API keys for the AI providers you want to use:
```bash
# At minimum, add one of these:
ANTHROPIC_API_KEY=sk-ant-...      # For Claude models
OPENAI_API_KEY=sk-...             # For GPT models
GROQ_API_KEY=gsk_...              # For fast Llama inference
```

**Don't have API keys?** The system works without them - you just won't be able to use AI features. Get keys from:
- [Anthropic Console](https://console.anthropic.com/)
- [OpenAI Platform](https://platform.openai.com/)
- [Groq Console](https://console.groq.com/)

### Step 3: Start the System

```bash
npm start
```

This will:
1. ✅ Build the TypeScript code (~30 seconds)
2. ✅ Start Rust workers for high-performance features
3. ✅ Launch the server (HTTP on :9000, WebSocket on :9001)
4. ✅ Open your browser to http://localhost:9000
5. ✅ Seed the database with default users and rooms

**First time?** The full startup takes ~2 minutes. Subsequent starts are faster (~90 seconds).

### Step 4: Verify It's Working

```bash
# Test the CLI
./jtag ping

# You should see:
# ✅ Server: ready with 143 commands
# ✅ Browser: connected
```

### Step 5: Try a Command

```bash
# Take a screenshot of the UI
./jtag screenshot

# Send a message to the chat
./jtag chat/send --room="general" --message="Hello from the CLI!"

# Export chat history
./jtag chat/export --room="general" --limit=20
```

## 📚 What You Get

### 1. Browser UI (http://localhost:9000)
- **Chat interface** with real-time messaging
- **AI personas** (Claude Code, Helper AI, Teacher AI, etc.)
- **Room system** for organizing conversations
- **User management** for both humans and AIs

### 2. Command-Line Interface (`./jtag`)
- **143 commands** for controlling the system
- **Type-safe** with full TypeScript support
- **Self-documenting** - run `./jtag help` or `./jtag list`
- **Scriptable** - use in automation and CI/CD

### 3. AI Team (if you added API keys)
- **Local personas** running on your machine (Ollama)
- **External AIs** (Claude, GPT, Groq, etc.)
- **Autonomous behavior** - AIs respond to relevant messages
- **Tool use** - AIs can run commands and interact with the system

## 🔧 Common Configuration

### Change Ports

Edit `~/.continuum/config.env`:
```bash
HTTP_PORT=3000  # Default: 9000
WS_PORT=3001    # Default: 9001
```

### Adjust Logging

```bash
LOG_LEVEL=debug        # debug, info, warn, error, silent
LOG_TO_CONSOLE=1       # Show logs in terminal (1) or hide them (0)
LOG_TO_FILES=1         # Write to .continuum/logs/ (1) or not (0)
LOG_FILE_MODE=clean    # clean (fresh), append (keep), archive (rotate)
```

### Custom Database Location

```bash
# Uncomment and set in ~/.continuum/config.env
DATABASE_DIR=/path/to/your/database
DATABASE_BACKUP_DIR=/path/to/backups
```

## 🛠️ Development Workflow

### Edit and Test Loop

```bash
# 1. Edit TypeScript files
# 2. Restart to see changes
npm start

# 3. Test with CLI or browser
./jtag ping
./jtag screenshot
```

**Important:** `npm start` rebuilds everything. Changes won't appear until you restart.

### Run Tests

```bash
# Integration tests (requires running server)
npx tsx tests/integration/crud.test.ts

# Unit tests
npm test
```

### Check Logs

```bash
# Server logs
tail -f .continuum/jtag/system/logs/npm-start.log

# Rust worker logs
tail -f .continuum/jtag/logs/system/rust-worker.log

# Or use the log command
./jtag logs/read --tailLines=50
```

## 📖 Next Steps

### Learn the Commands

```bash
# List all available commands
./jtag list

# Get help on a specific command
./jtag help screenshot
./jtag help chat/send

# See command schemas (for integration)
cat generated-command-schemas.json
```

### Explore the Chat System

```bash
# Send a message
./jtag chat/send --room="general" --message="What can you help me with?"

# Wait a few seconds for AI responses
sleep 10

# Export the conversation
./jtag chat/export --room="general" --limit=30

# Or view in browser at http://localhost:9000
```

### Build Something

The system is designed to be extended:
- **Add commands**: Create new commands in `commands/your-command/`
- **Add widgets**: Create UI components in `widgets/`
- **Add AI personas**: Configure new personas with different behaviors
- **Integrate with your app**: Import and use the JTAG client in your code

See the [Architecture Guide](docs/ARCHITECTURE-RULES.md) for details.

## 🆘 Troubleshooting

### "Command not found: jtag"

Make sure you're in the right directory:
```bash
cd /path/to/continuum/src/debug/jtag
./jtag ping  # Note the ./
```

### "Port already in use"

Change the ports in `~/.continuum/config.env` or kill the existing process:
```bash
lsof -ti:9000 | xargs kill
lsof -ti:9001 | xargs kill
```

### "Worker failed to start"

Check Rust is installed:
```bash
rustc --version
cargo --version

# If not installed:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### "Browser not connecting"

1. Check the browser console for errors
2. Verify the server is running: `./jtag ping`
3. Try a hard refresh: Cmd+Shift+R (Mac) or Ctrl+Shift+R (Windows)
4. Check firewall settings (ports 9000, 9001 must be open)

### Still stuck?

1. Check the logs: `tail -f .continuum/jtag/system/logs/npm-start.log`
2. Run with debug logging: Set `LOG_LEVEL=debug` in `~/.continuum/config.env`
3. File an issue with the error output

## 🎯 Goals and Philosophy

This system is designed to:
- **Make AI development interactive** - talk to your AI team like colleagues
- **Provide universal primitives** - `Commands.execute()` and `Events.emit()` work everywhere
- **Stay out of your way** - run in the background, integrate when you need it
- **Be self-documenting** - code and runtime metadata are the same

Welcome to the team! 🎉
