# Continuum - Universal Command API

> Designed by AI and humans for AI and humans

## Overview

Continuum provides an elegant, universal command API that enables seamless interaction between humans and AI agents. Commands are modular, self-contained, and dynamically discoverable.

## Command Structure

Each command is a self-contained ES module with:
- `package.json` - Module metadata and capabilities
- `index.server.js` - Server-side entry point
- Optional client-side components for browser integration

## Core Commands

### System Commands
- `test` - Run comprehensive test suites
- `help` - Display this help information
- `diagnostics` - System health and validation
- `info` - System information and status
- `clear` - Clear console/reset state
- `reload` - Reload system components
- `restart` - Restart system services

### File Operations
- `fileSave` - Save files to disk
- `saveFile` - Alternative file saving interface

### Agent Management
- `agents` - List and manage available agents
- `findUser` - Find and select users

### Development & Validation
- `validateCode` - Code validation and analysis
- `validateJS` - JavaScript-specific validation
- `exec` - Execute system commands
- `browserJS` - Execute JavaScript in browser context
- `promiseJS` - Promise-based JavaScript execution
- `macro` - Automation and macro functionality

### Communication
- `share` - Share content and files
- `screenshot` - Capture and save screenshots

### Configuration
- `preferences` - User preferences and settings

## Usage Patterns

### Basic Command Execution
```bash
continuum [command] [parameters]
```

### Fluent API (Future)
```javascript
continuum.screenshot().share(continuum.findUser({name:"joel"}))
```

## Command Discovery

Commands are automatically discovered from the `src/commands/core/` directory. Each command module's `package.json` defines its capabilities and metadata.

## Getting Help

- `continuum --help` - Show this help
- `continuum [command] --help` - Show command-specific help
- Each command module can include its own documentation files

## Architecture

Continuum uses a modular command architecture where:
1. Commands are ES modules with proper entry points
2. Server and client code is separated
3. Commands can compose and delegate to other commands
4. All functionality flows through the universal command API

This ensures consistent, predictable behavior across all interactions.