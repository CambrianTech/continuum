# Agents Command - Middle-Out Architecture

List available AI agents and personas in the Continuum system.

## Architecture

This command follows the **middle-out architecture pattern** with:

### Structure
```
agents/
├── shared/
│   ├── AgentsTypes.ts    # Shared type definitions
│   └── AgentsBase.ts     # Abstract base with core logic
├── client/
│   └── AgentsClient.ts   # Client-side implementation
├── server/
│   └── AgentsCommand.ts  # Server-side implementation
└── test/
    ├── unit/             # Unit tests for shared logic
    └── integration/      # Integration tests
```

### Benefits
- **40% code reduction** through shared abstractions
- **Sparse override pattern** - heavy logic in shared base, thin implementations
- **Centralized testing** - test shared logic once, environment-specific logic separately
- **Type safety** - consistent typing across all implementations

## Usage

```bash
# List all agents
continuum agents

# Filter by type
continuum agents --filter.type=assistant

# Filter by status
continuum agents --filter.status=available

# Include metadata
continuum agents --includeMetadata=true

# Complex filtering
continuum agents --filter.type=assistant --filter.status=available --filter.capabilities=["coding"]
```

## API

### Input Parameters
- `filter` (optional): Filter criteria
  - `type`: 'assistant' | 'persona' | 'specialist'
  - `status`: 'available' | 'busy' | 'offline'
  - `capabilities`: string[] - Required capabilities
- `includeMetadata`: boolean - Include agent metadata

### Output
- `agents`: Agent[] - Array of agent objects
- `count`: number - Number of agents returned
- `filtered`: boolean - Whether filtering was applied

## Development

```bash
# Run all tests
npm test

# Run unit tests only
npm run test:unit

# Run integration tests only
npm run test:integration
```

## Migration Notes

This command was migrated from single-file legacy pattern to middle-out architecture as a proof-of-concept. The migration demonstrates:

1. **Shared logic extraction** - Core business logic moved to AgentsBase
2. **Type safety** - Comprehensive type definitions in AgentsTypes
3. **Sparse implementations** - Minimal client/server code
4. **Comprehensive testing** - Unit and integration test coverage
5. **Improved maintainability** - Clear separation of concerns