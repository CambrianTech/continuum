# Multi-Database Security & Permissions

**Status**: FUTURE IMPLEMENTATION (Phase 2+)

**Current State**: Phase 1 implements basic handle system with NO security enforcement. All handles have full access to their databases.

**This Document**: Design for future security layer when multiple autonomous AIs need isolation.

---

## Problem

When you have multiple autonomous AIs (PersonaUsers) operating simultaneously:
- **Helper AI** shouldn't read **Teacher AI's** private memories
- **No AI** should delete production data (users, rooms, messages)
- **Training scripts** should have read-only access to source data
- **Fine-tuning jobs** need write access to output, but not input

Currently (Phase 1): All handles have god-mode access. Not a problem when humans control everything, but dangerous when AIs operate autonomously.

---

## Solution: Handle-Based Permissions

### Permission Model

Each `DbHandle` gets a permission set when opened:

```typescript
interface DbPermissions {
  read: boolean;      // Can read records
  write: boolean;     // Can create/update records
  delete: boolean;    // Can delete records
  schema: boolean;    // Can modify schema (create tables, indexes)

  // Collection-level restrictions
  allowedCollections?: string[];  // If specified, can only access these collections
  deniedCollections?: string[];   // Cannot access these collections

  // Row-level security
  rowFilter?: (record: any) => boolean;  // Only see rows that pass this filter

  // Ownership
  owner?: string;           // User/AI that owns this handle
  isolationMode?: 'none' | 'soft' | 'strict';
}
```

### Isolation Modes

**1. None** (default for Phase 1)
- No restrictions
- Full god-mode access
- Use for: Development, admin operations, trusted scripts

**2. Soft Isolation**
- Can read other personas' data
- Cannot write/delete other personas' data
- Use for: Collaborative AIs that need to see each other's work

**3. Strict Isolation**
- Cannot even open handles to other personas' databases
- Complete data isolation
- Use for: Paranoid personas, security-sensitive data

---

## Implementation Design

### Phase 2A: Permission Enforcement

Add permission checks to `DatabaseHandleRegistry`:

```typescript
export class DatabaseHandleRegistry {
  private handlePermissions: Map<DbHandle, DbPermissions>;

  /**
   * Open handle with permissions
   */
  async open(config: {
    adapter: string;
    config: any;
    permissions?: DbPermissions;  // NEW
    owner?: string;               // NEW
  }): Promise<DbHandle> {
    // Validate ownership for isolation modes
    if (config.permissions?.isolationMode === 'strict') {
      const dbOwner = this.getDatabaseOwner(config.config.path);
      if (dbOwner && dbOwner !== config.owner) {
        throw new Error(`Access denied: database owned by '${dbOwner}', requested by '${config.owner}'`);
      }
    }

    const handle = generateUUID();
    const adapter = await this.createAdapter(config);

    this.handles.set(handle, adapter);
    this.handlePermissions.set(handle, config.permissions || this.getDefaultPermissions());

    return handle;
  }

  /**
   * Check permission before operation
   */
  private checkPermission(handle: DbHandle, operation: 'read' | 'write' | 'delete' | 'schema'): void {
    const perms = this.handlePermissions.get(handle);
    if (!perms) return;  // No perms = allow (backward compatible)

    if (!perms[operation]) {
      throw new Error(`Permission denied: handle does not have ${operation} access`);
    }
  }

  /**
   * Check collection access
   */
  private checkCollectionAccess(handle: DbHandle, collection: string): void {
    const perms = this.handlePermissions.get(handle);
    if (!perms) return;

    if (perms.allowedCollections && !perms.allowedCollections.includes(collection)) {
      throw new Error(`Permission denied: collection '${collection}' not in allowed list`);
    }

    if (perms.deniedCollections && perms.deniedCollections.includes(collection)) {
      throw new Error(`Permission denied: collection '${collection}' is denied`);
    }
  }
}
```

### Phase 2B: Ownership Tracking

Database files store ownership metadata:

```
.continuum/personas/
├── helper-ai/
│   ├── knowledge.sqlite
│   └── .owner              # Contains: { "userId": "helper-ai", "createdAt": "...", "isolationMode": "strict" }
├── teacher-ai/
│   ├── knowledge.sqlite
│   └── .owner
```

### Phase 2C: Row-Level Security

For shared databases where multiple personas need different views:

```typescript
// Persona can only see their own facts
const handle = await Commands.execute('data/open', {
  adapter: 'sqlite',
  config: { path: '.continuum/shared/facts.sqlite' },
  owner: 'helper-ai',
  permissions: {
    read: true,
    write: true,
    delete: false,
    rowFilter: (record) => record.ownerId === 'helper-ai'  // Only see own rows
  }
});

// List returns ONLY rows where ownerId === 'helper-ai'
const myFacts = await Commands.execute('data/list', {
  dbHandle: handle,
  collection: 'facts'
});
```

---

## Common Permission Patterns

### Read-Only Training Data

```typescript
const trainingHandle = await Commands.execute('data/open', {
  adapter: 'sqlite',
  config: { path: '/datasets/prepared/continuum-git.sqlite' },
  permissions: {
    read: true,
    write: false,
    delete: false,
    schema: false
  }
});

// Can read
const examples = await Commands.execute('data/list', {
  dbHandle: trainingHandle,
  collection: 'training_examples'
});

// Cannot write
await Commands.execute('data/create', {
  dbHandle: trainingHandle,
  collection: 'training_examples',
  data: example
});
// Error: Permission denied - handle does not have write access
```

### Persona Knowledge Base (Strict Isolation)

```typescript
const helperKB = await Commands.execute('data/open', {
  adapter: 'sqlite',
  config: { path: '.continuum/personas/helper-ai/knowledge.sqlite' },
  owner: 'helper-ai',
  permissions: {
    read: true,
    write: true,
    delete: true,
    schema: false,
    isolationMode: 'strict'
  }
});

// Helper AI can read/write its own KB
await Commands.execute('data/create', {
  dbHandle: helperKB,
  collection: 'facts',
  data: { fact: 'TypeScript is awesome', confidence: 0.9 }
});

// Teacher AI CANNOT open Helper's KB
const teacherAttempt = await Commands.execute('data/open', {
  adapter: 'sqlite',
  config: { path: '.continuum/personas/helper-ai/knowledge.sqlite' },
  owner: 'teacher-ai',
  permissions: { isolationMode: 'strict' }
});
// Error: Access denied - database owned by 'helper-ai', requested by 'teacher-ai'
```

### Fine-Tuning Job (Write Output Only)

```typescript
const outputHandle = await Commands.execute('data/open', {
  adapter: 'sqlite',
  config: { path: '/datasets/adapters/typescript-expert/metrics.sqlite' },
  permissions: {
    read: true,
    write: true,
    delete: false,
    schema: true,  // Can create tables for metrics
    allowedCollections: ['training_metrics', 'evaluation_results']
  }
});

// Can write metrics
await Commands.execute('data/create', {
  dbHandle: outputHandle,
  collection: 'training_metrics',
  data: { epoch: 1, loss: 0.42 }
});

// Cannot access main database collections
await Commands.execute('data/list', {
  dbHandle: outputHandle,
  collection: 'users'  // Not in allowedCollections
});
// Error: Permission denied - collection 'users' not allowed for this handle
```

---

## Audit Logging (Phase 2D)

All data operations get logged for security auditing:

```typescript
interface AuditLogEntry {
  timestamp: number;
  handle: DbHandle;
  owner: string;
  operation: 'read' | 'write' | 'delete' | 'schema';
  collection: string;
  recordId?: string;
  success: boolean;
  error?: string;
}

// Stored in: .continuum/jtag/security/audit.log
```

View audit logs:
```bash
./jtag data/audit --owner=helper-ai --since="1 hour ago"
```

---

## Permission Presets

Common permission sets for convenience:

```typescript
export const PERMISSION_PRESETS = {
  // Full access (Phase 1 default)
  GOD_MODE: {
    read: true,
    write: true,
    delete: true,
    schema: true
  },

  // Read-only access
  READ_ONLY: {
    read: true,
    write: false,
    delete: false,
    schema: false
  },

  // Training job (can write metrics but not delete)
  TRAINING_JOB: {
    read: true,
    write: true,
    delete: false,
    schema: true,
    allowedCollections: ['training_metrics', 'checkpoints', 'evaluation_results']
  },

  // Persona KB (strict isolation)
  PERSONA_KNOWLEDGE: {
    read: true,
    write: true,
    delete: true,
    schema: false,
    isolationMode: 'strict'
  },

  // Shared collaborative space
  COLLABORATIVE: {
    read: true,
    write: true,
    delete: false,
    schema: false,
    isolationMode: 'soft'
  }
};

// Usage:
const handle = await Commands.execute('data/open', {
  adapter: 'sqlite',
  config: { path: '/datasets/training.sqlite' },
  permissions: PERMISSION_PRESETS.READ_ONLY
});
```

---

## Migration Path

**Phase 1** (Current):
- Implement handle system with NO security
- All handles have god-mode access
- Focus on functionality, not security

**Phase 2A** (When AIs get autonomy):
- Add permission enforcement
- Implement ownership tracking
- Add collection-level restrictions

**Phase 2B** (When paranoia justified):
- Strict isolation modes
- Row-level security
- Audit logging

**Phase 2C** (When breaches occur):
- Encryption at rest
- Handle expiration/revocation
- Multi-factor authentication for sensitive operations

---

## Testing Strategy

```typescript
describe('Multi-Database Security', () => {
  it('should enforce read-only permissions', async () => {
    const handle = await Commands.execute('data/open', {
      adapter: 'sqlite',
      config: { path: '/tmp/test.sqlite' },
      permissions: PERMISSION_PRESETS.READ_ONLY
    });

    await expect(
      Commands.execute('data/create', {
        dbHandle: handle,
        collection: 'test',
        data: { foo: 'bar' }
      })
    ).rejects.toThrow('Permission denied');
  });

  it('should enforce strict isolation', async () => {
    // Create database owned by helper-ai
    await fs.writeFile(
      '.continuum/personas/helper-ai/.owner',
      JSON.stringify({ userId: 'helper-ai' })
    );

    // Teacher AI tries to open it
    await expect(
      Commands.execute('data/open', {
        adapter: 'sqlite',
        config: { path: '.continuum/personas/helper-ai/knowledge.sqlite' },
        owner: 'teacher-ai',
        permissions: { isolationMode: 'strict' }
      })
    ).rejects.toThrow('Access denied');
  });
});
```

---

## Key Takeaways

1. **Phase 1**: Build it, make it work, no security
2. **Phase 2**: Add security when AIs become autonomous
3. **Phase 3**: Add paranoia features when needed

**Current Rule**: Be careful, use common sense, don't let AIs run wild yet.

**Future Rule**: Enforce everything, trust no one, audit everything.

---

## Notes for Future Implementer

- Permission checks happen in `DatabaseHandleRegistry.getAdapter()` before returning adapter
- All `data/*` commands already pass through registry, so enforcement is centralized
- Backward compatible: if `permissions` undefined, use god-mode (Phase 1 behavior)
- Ownership metadata stored in `.owner` files next to databases
- Audit log is append-only SQLite database (yes, a database for database security!)

Remember: **Autonomous AIs are toddlers with root access.** Plan accordingly. 🧸🔓
