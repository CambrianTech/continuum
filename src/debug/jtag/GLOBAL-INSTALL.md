# JTAG Global CLI Installation

## Install Once, Use Everywhere

Like Claude Code, JTAG is designed as a global CLI tool that works from any directory:

```bash
# Install globally (one time)
npm install -g @continuum/jtag

# Use from any project directory
cd /your/project
jtag screenshot
jtag ping
jtag help
```

## How It Works

**Global Installation Benefits:**
- ✅ **Universal Access**: `jtag` command available from any directory
- ✅ **No Version Conflicts**: Single global installation, no local dependencies
- ✅ **Clean Project Directories**: No need to modify package.json files
- ✅ **Consistent Experience**: Same commands work everywhere

**Directory Behavior:**
- Creates `.continuum/jtag/` in whatever directory you run it from
- Screenshots saved to `.continuum/jtag/currentUser/screenshots/`
- Logs saved to `.continuum/jtag/currentUser/logs/`
- Each project gets its own isolated JTAG workspace

## Usage Examples

```bash
# Install once globally
npm install -g @continuum/jtag

# Use in any project
cd ~/my-react-app
jtag screenshot --filename=react-debug.png

cd ~/my-node-api  
jtag screenshot --filename=api-debug.png

cd ~/my-python-project
jtag screenshot --filename=python-debug.png
```

## Architecture

**Global CLI Pattern:**
```
/usr/local/lib/node_modules/@continuum/jtag/  # Global installation
├── dist/cli.js                               # CLI entry point
├── dist/server-index.js                      # Server system
└── dist/browser-index.js                     # Browser client

/your/project/.continuum/jtag/                # Per-project workspace
├── currentUser/screenshots/                  # Your screenshots
├── currentUser/logs/                         # Debug logs
└── system/                                   # System files
```

**Like Claude Code:**
- Single global installation
- Works from any directory
- Creates project-specific workspaces
- No configuration required
- Just works!

## Development Mode

For JTAG development itself, you can still use local mode:

```bash
cd /path/to/jtag/source
JTAG_WORKING_DIR="examples/test-bench" npm start
```

But users should always use the global installation pattern.