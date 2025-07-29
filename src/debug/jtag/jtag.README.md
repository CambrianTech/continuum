# JTAG CLI Design

## How It Actually Works

The JTAG CLI system is designed to be simple and self-contained:

### CLI Architecture

1. **Entry Point**: `./jtag` script in `src/debug/jtag/`
2. **CLI Handler**: Forwards to `src/debug/jtag/cli/` 
3. **System Launcher**: CLI calls `npm start` within `src/debug/jtag/`
4. **Full System**: `npm start` launches all of Continuum and keeps it running
5. **Browser Auto-Launch**: Browser gets launched automatically if not already up

### Usage

```bash
cd src/debug/jtag
./jtag screenshot --filename test.png
```

### What Happens

1. `./jtag` forwards the command to the CLI system
2. CLI system ensures `npm start` is running (launches all of Continuum)
3. Browser is automatically launched if needed
4. Screenshot command executes through the running system
5. System stays running for future commands

### Key Points

- **Server-side CLI**: The CLI runs from server side and manages the full system
- **Self-Managing**: System launches browser automatically when needed  
- **Persistent**: `npm start` keeps everything running between commands
- **Simple**: Just call `./jtag <command>` and it handles the rest

This design eliminates the need to manually start systems or manage browser connections - the CLI handles all of that automatically.

## Analogy: Just Like `continuum screenshot`

It works exactly like calling `Bash(continuum screenshot)` in our test examples:

- **Test-bench example**: `continuum screenshot` → takes full screen image → returns filename
- **JTAG CLI**: `./jtag screenshot --filename test.png` → takes screenshot → saves file

Both are seamless - you just call the command and get the result. The difference is:
- `continuum screenshot` runs from browser/test environment  
- `./jtag screenshot` runs from server-side CLI but connects to the same system

Same underlying screenshot functionality, different entry points.

## JTAG: A Terminal to the Entire System

This is literally like a **real JTAG unit** for the entire Continuum development system:

### How It Works
1. **`npm start` does the heavy lifting** - launches the full Continuum system
2. **CLI checks if system is running** - starts it if needed  
3. **Connects to session** - defaults to user browser session but has access to everything
4. **Commands execute across the entire system** - browser, server, file system, all of it

### System-Wide Control
- All CLI commands can interact with the entire Continuum system
- Browser commands: screenshot, click, navigate, type
- Server commands: file operations, compilation, system status
- Cross-system commands: chat, session management, debugging
- It's like having a terminal directly connected to your entire development environment

### Real JTAG Analogy
Just like a hardware JTAG unit gives you complete system access for embedded debugging, this gives you complete system access for web development.

**Hardware JTAG**: Terminal → JTAG Unit → Entire Embedded System  
**Continuum JTAG**: CLI → JTAG System → Entire Development System (Browser + Server + Files + Everything)