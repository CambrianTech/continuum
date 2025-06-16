# Universal Command Architecture

## Core Philosophy: Dogmatic Simplicity

Every client (JavaScript, Python, AI agents) should use the same elegant pattern:

```javascript
// JavaScript client
continuum.command.screenshot()
continuum.command.screenshot({selector: '.version-badge'})
continuum.command.screenshot().manual()
```

```python
# Python client
await continuum.command.screenshot()
await continuum.command.screenshot(selector='.version-badge')
await continuum.command.screenshot().manual()
```

```
AI Agent output:
continuum.command.screenshot({selector: 'body'})
```

## Architecture Layers

### 1. Core Command Engine (Daemon Level)
- All logic lives in `src/commands/`
- Commands are stateless, pure functions
- Elegant input/output interfaces
- Built-in error handling and validation

### 2. Universal Client Interface
- Same API across all languages/contexts
- Promise-based responses
- Automatic retry and timeout handling
- Built-in help and discovery

### 3. AI Agent Integration
- Commands parseable from natural language output
- Rich response objects for AI consumption
- Screenshot visibility and collaboration features
- Help/interface discovery for autonomous learning

## Command Structure

```javascript
class ScreenshotCommand extends BaseCommand {
  // Core logic (daemon level)
  static async execute(params, continuum) {
    // All screenshot logic here - no client-side complexity
  }
  
  // Universal interface definitions
  static getInterface() {
    return {
      javascript: 'continuum.command.screenshot(options)',
      python: 'await continuum.command.screenshot(options)',
      ai_output: 'continuum.command.screenshot({selector: "..."})'
    }
  }
}
```

## Implementation Plan

1. **Test Current State** - Ensure system stability
2. **Core Command Logic** - Move all screenshot logic to daemon
3. **Universal Client APIs** - Identical interfaces across languages
4. **AI Agent Integration** - Screenshot visibility and command parsing
5. **Comprehensive Testing** - Baby steps with validation

## Benefits

- **Zero Cognitive Waste** - Same pattern everywhere
- **AI-Friendly** - Easy for agents to learn and use
- **Maintainable** - Logic centralized in daemon
- **Discoverable** - Built-in help and interfaces
- **Collaborative** - Screenshots visible to all agents