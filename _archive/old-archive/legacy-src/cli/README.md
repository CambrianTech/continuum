# Continuum CLI Module

## Description

Ultra-thin CLI pipe that knows NOTHING about commands.

## Architecture

**Non-God Object Pattern**: The CLI forwards all arguments directly to command modules without any knowledge of command interfaces, parameters, or behavior.

## Responsibilities

- Extract command name from first argument
- Forward raw arguments as `{ "args": [...] }`
- HTTP POST to command API endpoint
- Pretty print JSON responses
- Check daemon availability

## What CLI Does NOT Do

- ❌ Parse command-specific parameters
- ❌ Validate command arguments  
- ❌ Know about command interfaces
- ❌ Handle command-specific logic
- ❌ Provide command-specific help

## What Command Modules Do

- ✅ Define parameter interfaces
- ✅ Parse and validate arguments
- ✅ Handle `--help` requests
- ✅ Execute command logic
- ✅ Format responses

## Usage

```bash
# CLI just forwards everything to the command module:
continuum [command] [arguments...]

# Examples (CLI knows nothing about these parameters):
continuum health
continuum screenshot --filename=test.png
continuum data-marshal --operation=encode --data='{}'
```

## Adding New Commands

Adding new commands requires ZERO CLI changes. Just create a new command module with proper `getDefinition()` and `execute()` methods.

## Module Compliance

This CLI module follows the universal modular architecture:
- ✅ Self-contained with package.json
- ✅ Minimal responsibility scope
- ✅ No cross-cutting concerns
- ✅ Clean interfaces
- ✅ Testable components