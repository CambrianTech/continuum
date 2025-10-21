# Legacy Continuum Source Code (Archived)

**Archived Date**: October 21, 2025
**Reason**: Promoting `src/debug/jtag` to become the main Continuum package

## What is This?

This directory contains the original Continuum system that existed in `src/` before the JTAG (Joel's Testing And Governance) system was developed. The JTAG system started as a debugging/testing tool in `src/debug/jtag/` but has evolved to become the new, production-ready Continuum implementation.

## Directory Structure

The archived legacy system includes:

- **academy/** - Original AI training/learning system
- **chat/** - Original chat implementation
- **cli/** - Original command-line interface
- **commands/** - Legacy command system
- **context/** - Context management
- **core/** - Core system functionality
- **daemons/** - Legacy daemon processes
- **integrations/** - External service integrations
- **monitoring/** - System monitoring tools
- **parsers/** - Message/content parsers
- **services/** - Legacy service layer
- **shared/** - Shared utilities and types
- **system/** - System-level infrastructure
- **test/** - Legacy test infrastructure
- **testing/** - Test utilities
- **types/** - TypeScript type definitions
- **ui/** - Original UI components
- **hot-reload.ts** - Hot module reloading
- **MULTI_PROCESS_DESIGN.md** - Original multi-process architecture documentation

## Why Was This Archived?

The JTAG system (`src/debug/jtag/`) represents a complete reimagining of Continuum with several key improvements:

1. **Rust-Like Type Safety**: Strict TypeScript without `any` types
2. **Better Architecture**: Cleaner separation of concerns with shared/browser/server patterns
3. **Event-Driven**: Proper real-time event system with server-originated events
4. **Modern Testing**: Comprehensive CRUD + state integration tests
5. **Widget System**: Clean Shadow DOM-based widget architecture
6. **Command System**: Type-safe command execution with proper parameter validation
7. **Data Layer**: Elegant entity system with versioning and real-time sync
8. **AI Integration**: Proper AI user types (PersonaUser, AgentUser) with genomic capabilities

## Migration Status

- **Current State**: Legacy code archived to `archive/legacy-src/`
- **Active System**: `src/debug/jtag/` (soon to be promoted to `src/`)
- **Next Steps**:
  - Move JTAG code from `src/debug/jtag/` to `src/`
  - Update package.json and build configuration
  - Archive remaining debugging artifacts

## Reference

This code remains available for:
- Historical reference
- Understanding design evolution
- Recovering any missed functionality
- Documentation of the original architecture

**Note**: This code is not maintained and should not be used for new development. All new work should be done in the JTAG system.
