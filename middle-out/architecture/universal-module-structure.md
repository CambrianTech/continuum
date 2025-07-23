# Universal Module Structure

## 🏗️ `/shared|browser|server|remote` Pattern

All Continuum modules should follow this universal structure pattern, regardless of module type (daemons, commands, widgets, continuum core, etc.).

### 📁 Structure Pattern

```
module-name/
├── shared/     # Common code, types, utilities used across contexts
├── browser/    # Browser/frontend code
├── server/     # Node.js server-side code
├── remote/     # AWS Lambda/serverless functions for distributed execution
├── tests/      # Test files organized by context
└── package.json
```

### 🎯 Context Definitions

#### `/shared` - Universal Context
- **Purpose**: Code that runs in ANY context
- **Contents**: 
  - Common types and interfaces
  - Validation logic
  - Protocol definitions
  - Utilities that work everywhere
  - Constants and enums
- **Examples**: `MessageProtocol.ts`, `ValidationRules.ts`, `SharedTypes.ts`

#### `/browser` - Browser Context
- **Purpose**: Code that runs in the browser
- **Contents**:
  - UI components and widgets
  - Browser-specific APIs
  - DOM manipulation
  - Client-side event handling
  - WebSocket client connections
- **Examples**: `ChatWidget.ts`, `BrowserManager.ts`, `DOMUtils.ts`

#### `/server` - Node.js Context
- **Purpose**: Code that runs on the local server/daemon
- **Contents**:
  - Daemon implementations
  - Server-side business logic
  - File system operations
  - Local database connections
  - IPC and WebSocket servers
- **Examples**: `SessionManager.ts`, `FileHandler.ts`, `DaemonCore.ts`

#### `/remote` - Distributed Context
- **Purpose**: Code that runs on remote infrastructure
- **Contents**:
  - Remote execution orchestration
  - Network mesh coordination
  - Distributed state management
  - Cross-network communication
  - Integration-specific implementations
- **Structure**:
  ```
  remote/
  ├── shared/          # Remote-specific shared types
  ├── server/          # Remote server orchestration
  └── integrations/    # External system integrations
      ├── aws-lambda/
      │   ├── shared/
      │   └── remote/
      ├── p2p-mesh/
      │   ├── shared/
      │   └── remote/
      └── azure-functions/
          ├── shared/
          └── remote/
  ```
- **Examples**: `RemoteOrchestrator.ts`, `NetworkMesh.ts`, `P2PIntegration.ts`

### 🔧 Module Type Applications

#### Daemons
```
src/daemons/session-manager/
├── shared/     # Session types, protocols
├── browser/    # Browser session UI
├── server/     # Session daemon logic
├── remote/     # Distributed session sync
│   ├── shared/
│   ├── server/
│   └── integrations/
│       ├── p2p-mesh/
│       └── cloud-sync/
└── tests/      # Context-specific tests
```

#### Commands
```
src/commands/screenshot/
├── shared/     # Screenshot types, validation
├── browser/    # Browser screenshot capture
├── server/     # Server screenshot coordination
├── remote/     # Cloud screenshot processing
│   ├── shared/
│   ├── server/
│   └── integrations/
│       ├── aws-lambda/
│       └── p2p-mesh/
└── tests/      # Command testing
```

#### Widgets
```
src/ui/components/Chat/
├── shared/     # Chat message types, protocols
├── browser/    # ChatWidget UI component
├── server/     # Chat message processing
├── remote/     # Distributed chat routing
│   ├── shared/
│   ├── server/
│   └── integrations/
│       ├── p2p-mesh/
│       └── chat-relay/
└── tests/      # Widget testing
```

#### Continuum Core
```
src/
├── shared/     # Core types, protocols
├── browser/    # Browser integration
├── server/     # Daemon system
├── remote/     # Distributed compute
│   ├── shared/
│   ├── server/
│   └── integrations/
│       ├── aws-lambda/
│       ├── p2p-mesh/
│       └── azure-functions/
└── tests/      # System tests
```

### 🧪 Testing Structure

Tests should mirror the module structure:

```
tests/
├── shared/     # Tests for shared code
├── browser/    # Browser/UI tests
├── server/     # Server/daemon tests
├── remote/     # Distributed/P2P tests
└── integration/ # Cross-context tests
```

### 🔗 Cross-Context Communication

Modules communicate across contexts through:
- **Shared protocols** defined in `/shared`
- **WebSocket connections** between browser/server
- **P2P networking** for remote contexts
- **Event-driven messaging** for loose coupling

### 📋 Implementation Guidelines

1. **Start with `/shared`** - Define types and protocols first
2. **Build `/server`** - Implement core business logic
3. **Create `/browser`** - Build user interface components
4. **Plan `/remote`** - Design distributed components
5. **Test thoroughly** - Ensure all contexts work together

### 🎯 Benefits

- **Clear separation of concerns**
- **Easier reasoning about execution contexts**
- **Better code reusability**
- **Simplified testing strategies**
- **Future-proof for distributed architecture**
- **P2P connectivity preparation**

### 🚀 Migration Strategy

For existing modules:
1. Identify current code by execution context
2. Create `/shared|browser|server|remote` structure
3. Move code to appropriate contexts
4. Update imports and dependencies
5. Verify tests still pass
6. Document context-specific behavior

This structure supports the vision of distributed P2P collaboration while maintaining clear architectural boundaries.