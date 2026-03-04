# Wall Commands & Lease System — Implementation Architecture

> Complete implementation architecture for wall/write, wall/read, wall/list commands with lease-based democratic file access control.

**Parent:** [Activities](README.md)

**Status**: Implementation Phase
**Version**: 1.0  
**Date**: 2025-12-03  
**Author**: Claude Code (Memento)

---

## Executive Summary

This document provides the complete implementation architecture for:
1. **Room Wall Commands** (wall/write, wall/read, wall/list) - Collaborative document space
2. **Lease System** (lease/request, lease/vote-kick) - Democratic file access control
3. **AI Tool Access** - Enabling PersonaUsers to use these commands

### Implementation Strategy

**Phase 1: Minimal Viable Wall System** (Current Focus)
- Simple file-based wall commands WITHOUT lease system
- Direct writes with git commits
- Enable for AI use immediately

**Phase 2: Lease Integration** (Future)
- Add FileLease entity support
- Implement LeaseDaemon
- Democratic governance (kick voting, peer approval)

---

## Directory Structure

### Storage Location
```
.continuum/shared/rooms/
├── {room-uuid}/
│   ├── governance-framework.md
│   ├── dashboard-design.md
│   └── [other-docs].md
└── {room-uuid}/
    └── [docs].md
```

**Key Design Decisions:**
- Use `.continuum/shared/rooms/{room-uuid}/` NOT `.continuum/shared/rooms/{room-name}/`
- UUID-based directories ensure uniqueness and stability
- Room name lookup happens via RoomEntity query
- Documents are markdown files (`.md` extension enforced)

---

## Entity Architecture

### 1. WallDocumentEntity - Metadata Storage

**Location**: `system/data/entities/WallDocumentEntity.ts`

**Purpose**: Database record for wall document metadata

**Fields:**
```typescript
export class WallDocumentEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.WALL_DOCUMENTS;

  // Room association
  @TextField({ index: true })
  roomId!: UUID;

  // Document identity
  @TextField({ index: true })
  name!: string;                    // e.g., "governance-framework.md"

  // File storage location
  @TextField()
  filePath!: string;                // Absolute path to content file

  // Authorship and timing
  @TextField({ index: true })
  createdBy!: UUID;                 // Original author

  @TextField({ index: true })
  lastModifiedBy!: UUID;            // Last editor

  @DateField({ index: true })
  createdAt!: Date;

  @DateField({ index: true })
  lastModifiedAt!: Date;

  // File statistics (cached from filesystem)
  @TextField()
  lineCount!: number;

  @TextField()
  byteCount!: number;

  // Git tracking
  @TextField({ nullable: true })
  lastCommitHash?: string;

  // Future: Lease integration
  @TextField({ nullable: true, index: true })
  currentLeaseId?: UUID;            // Active lease if any

  @TextField({ nullable: true })
  protectionLevel?: 'unrestricted' | 'peer_review' | 'senior_review' | 'human_review';
}
```

### 2. WallManager Utility Class

**Location**: `system/storage/core/WallManager.ts`

**Purpose**: Orchestrates Commands.execute() calls for wall operations

**Responsibilities:**
- Resolve room name → room UUID → file path
- Coordinate entity CRUD with file operations
- Use file/save, file/load commands for content
- Update WallDocumentEntity metadata
- Emit room-scoped events
- Generate TOC from markdown content

**Architecture Pattern - Two-Layer Storage:**
```
WallDocumentEntity (database)     File Content (filesystem)
├── id, roomId, name               ├── Actual markdown text
├── filePath                       ├── Managed by file/* commands
├── createdBy, lastModifiedBy      └── Git tracked via git commands
├── lineCount, byteCount
└── lastCommitHash
```

**Key Methods:**
```typescript
class WallManager {
  // Room resolution (queries RoomEntity via data/list)
  async resolveRoomPath(roomNameOrId: UUID | string): Promise<RoomPathInfo>

  // Document write (entity + file + git + event)
  async writeDocument(
    room: UUID | string,
    doc: string,
    content: string,
    append?: boolean,
    author?: UUID
  ): Promise<WriteResult>

  // Document read (entity + file)
  async readDocument(
    room: UUID | string,
    doc: string,
    startLine?: number,
    endLine?: number
  ): Promise<ReadResult>

  // List documents (query WallDocumentEntity)
  async listDocuments(
    room: UUID | string,
    pattern?: string
  ): Promise<WallDocumentInfo[]>

  // Utilities
  async generateTOC(content: string): Promise<TOCEntry[]>
  sanitizeDocName(doc: string): string
}
```

### 2. Command Implementations

#### wall/write Command

**Files:**
```
commands/wall/write/
├── shared/
│   ├── WallWriteTypes.ts    (exists)
│   └── WallWriteCommand.ts  (base class - TO CREATE)
└── server/
    └── WallWriteServerCommand.ts (implementation - TO CREATE)
```

**Implementation Flow:**
1. Receive params (room, doc, content, append, author, commitMessage)
2. Resolve room name/UUID → RoomEntity
3. Sanitize document name
4. Use WallManager to write file
5. Auto-commit to git with author attribution
6. Return file stats and commit hash

#### wall/read Command

**Files:**
```
commands/wall/read/
├── shared/
│   ├── WallReadTypes.ts    (exists)
│   └── WallReadCommand.ts  (base class - TO CREATE)
└── server/
    └── WallReadServerCommand.ts (implementation - TO CREATE)
```

**Implementation Flow:**
1. Receive params (room, doc, toc, startLine, endLine, lines)
2. Resolve room → file path
3. If `toc=true`: Generate table of contents from markdown headers
4. If line range specified: Read only those lines
5. Otherwise: Read entire file
6. Return content + optional metadata

**TOC Generation:**
```typescript
// For markdown:
## Heading 1 (line 5)
### Subheading 1.1 (line 23)
## Heading 2 (line 58)

// TOC output:
[
  { level: 2, title: "Heading 1", line: 5 },
  { level: 3, title: "Subheading 1.1", line: 23 },
  { level: 2, title: "Heading 2", line: 58 }
]
```

#### wall/list Command

**Files:**
```
commands/wall/list/
├── shared/
│   ├── WallListTypes.ts    (exists)
│   └── WallListCommand.ts  (base class - TO CREATE)
└── server/
    └── WallListServerCommand.ts (implementation - TO CREATE)
```

**Implementation Flow:**
1. Receive params (room, pattern)
2. Resolve room → directory path
3. List all `.md` files (with optional glob pattern)
4. For each file, get stats (lines, bytes, last modified, last author)
5. Return sorted list of WallDocument objects

---

## Data Flow Diagrams

### wall/write Flow (Correct Architecture)
```
AI/Human → CLI → Commands.execute<WallWriteParams, WallWriteResult>('wall/write', params)
                      ↓
            WallWriteServerCommand.execute()
                      ↓
            WallManager.resolveRoomPath(room)
                      ↓
            Commands.execute<DataListParams, DataListResult>('data/list', {
              collection: 'rooms',
              filter: {name: room}
            })
                      ↓
            WallManager.writeDocument(roomId, doc, content, append, author)
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 1. Check if document exists in database     │
            │    Commands.execute('data/list', {          │
            │      collection: 'wall_documents',          │
            │      filter: {roomId, name: doc}            │
            │    })                                       │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 2. Load existing content if append=true     │
            │    Commands.execute<FileLoadParams,         │
            │      FileLoadResult>('file/load', {         │
            │        filepath: wallPath                   │
            │      })                                     │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 3. Save file content                        │
            │    Commands.execute<FileSaveParams,         │
            │      FileSaveResult>('file/save', {         │
            │        filepath: wallPath,                  │
            │        content: finalContent,               │
            │        createDirs: true                     │
            │      })                                     │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 4. Git commit (if git commands exist)       │
            │    Commands.execute('git/commit', {         │
            │      filepath: wallPath,                    │
            │      message: commitMsg,                    │
            │      author: author                         │
            │    })                                       │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 5. Create/update WallDocumentEntity         │
            │    Commands.execute<DataCreateParams,       │
            │      DataCreateResult>('data/create', {     │
            │        collection: 'wall_documents',        │
            │        data: {                              │
            │          roomId, name, filePath,            │
            │          createdBy, lastModifiedBy,         │
            │          lineCount, byteCount,              │
            │          lastCommitHash                     │
            │        }                                    │
            │      })                                     │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 6. Emit room-scoped event                   │
            │    Events.emit(`wall:document:${roomId}`, { │
            │      room, roomId, doc, author,             │
            │      action, summary, preview               │
            │    })                                       │
            └─────────────────────────────────────────────┘
                      ↓
            Return WallWriteResult {success, filePath, commitHash, stats}
```

### wall/read with TOC Flow (Correct Architecture)
```
AI/Human → CLI → Commands.execute<WallReadParams, WallReadResult>('wall/read', {
                      room, doc, toc: true
                    })
                      ↓
            WallReadServerCommand.execute()
                      ↓
            WallManager.resolveRoomPath(room)
                      ↓
            Commands.execute<DataListParams, DataListResult>('data/list', {
              collection: 'rooms',
              filter: {name: room}
            })
                      ↓
            WallManager.readDocument(roomId, doc)
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 1. Query WallDocumentEntity for metadata    │
            │    Commands.execute('data/list', {          │
            │      collection: 'wall_documents',          │
            │      filter: {roomId, name: doc}            │
            │    })                                       │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 2. Load file content                        │
            │    Commands.execute<FileLoadParams,         │
            │      FileLoadResult>('file/load', {         │
            │        filepath: entity.filePath            │
            │      })                                     │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 3. Apply line range if specified            │
            │    content.split('\n').slice(start, end)    │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 4. Generate TOC if requested                │
            │    WallManager.generateTOC(content)         │
            │    Parse: /^(#{1,6})\s+(.+)$/               │
            └─────────────────────────────────────────────┘
                      ↓
            Return WallReadResult {content, metadata, toc}
```

### wall/list Flow (Correct Architecture)
```
AI/Human → CLI → Commands.execute<WallListParams, WallListResult>('wall/list', {
                      room, pattern
                    })
                      ↓
            WallListServerCommand.execute()
                      ↓
            WallManager.resolveRoomPath(room)
                      ↓
            Commands.execute<DataListParams, DataListResult>('data/list', {
              collection: 'rooms',
              filter: {name: room}
            })
                      ↓
            WallManager.listDocuments(roomId, pattern)
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 1. Query all WallDocumentEntity records     │
            │    Commands.execute('data/list', {          │
            │      collection: 'wall_documents',          │
            │      filter: {roomId},                      │
            │      orderBy: [{field: 'lastModifiedAt',    │
            │                 direction: 'desc'}]         │
            │    })                                       │
            └─────────────────────────────────────────────┘
                      ↓
            ┌─────────────────────────────────────────────┐
            │ 2. Apply pattern filter if specified        │
            │    Filter by glob pattern matching name     │
            └─────────────────────────────────────────────┘
                      ↓
            Return WallListResult {documents: [...], count}
```

---

## Implementation Checklist

### Phase 1: Minimal Viable Wall System (Entity-Based Architecture)

**1. Create WallDocumentEntity** (`system/data/entities/WallDocumentEntity.ts`)
- [ ] Define entity with proper decorators (@TextField, @DateField)
- [ ] Fields: roomId, name, filePath, createdBy, lastModifiedBy, createdAt, lastModifiedAt
- [ ] Fields: lineCount, byteCount, lastCommitHash
- [ ] Optional fields for lease integration (currentLeaseId, protectionLevel)
- [ ] Implement validate() method
- [ ] Add to COLLECTIONS in DatabaseConfig
- [ ] Register in EntityRegistry

**2. Refactor WallManager utility** (`system/storage/core/WallManager.ts`)
- [ ] Replace all `fs.readFile()` with `Commands.execute<FileLoadParams, FileLoadResult>('file/load', ...)`
- [ ] Replace all `fs.writeFile()` with `Commands.execute<FileSaveParams, FileSaveResult>('file/save', ...)`
- [ ] Replace all execSync git calls with git commands (if available)
- [ ] Use `Commands.execute<DataListParams, DataListResult>('data/list', ...)` for entity queries
- [ ] Use `Commands.execute<DataCreateParams, DataCreateResult>('data/create', ...)` for entity creation
- [ ] Keep generateTOC() as utility (no I/O)
- [ ] Keep sanitizeDocName() as utility (no I/O)
- [ ] Emit room-scoped events via Events.emit()

**3. Refactor wall/write command**
- [ ] Update `commands/wall/write/server/WallWriteServerCommand.ts`
- [ ] Use refactored WallManager (Commands.execute only)
- [ ] Auto-fill author from session user if not provided
- [ ] Return file stats and commit hash from WallDocumentEntity

**4. Implement wall/read command**
- [ ] Create `commands/wall/read/shared/WallReadCommand.ts` (base class)
- [ ] Create `commands/wall/read/server/WallReadServerCommand.ts` (implementation)
- [ ] Query WallDocumentEntity for metadata
- [ ] Use Commands.execute('file/load') for content
- [ ] Support TOC-only mode
- [ ] Support line range reading
- [ ] Include metadata from entity

**5. Implement wall/list command**
- [ ] Create `commands/wall/list/shared/WallListCommand.ts` (base class)
- [ ] Create `commands/wall/list/server/WallListServerCommand.ts` (implementation)
- [ ] Query WallDocumentEntity via Commands.execute('data/list')
- [ ] Support glob pattern filtering (in-memory)
- [ ] Return entity data (no additional file reads needed)
- [ ] Sort by lastModifiedAt (via orderBy in data/list)

**6. Register commands**
- [ ] Add wall/write to command registry
- [ ] Add wall/read to command registry
- [ ] Add wall/list to command registry
- [ ] Verify Commands.execute() routes correctly

**7. Enable for AI use**
- [ ] Find PersonaToolExecutor tool whitelist/configuration
- [ ] Add wall/write to allowed tools
- [ ] Add wall/read to allowed tools
- [ ] Add wall/list to allowed tools
- [ ] Document tool usage in AI tool catalog

**8. Testing**
- [ ] Run `npm run build:ts` - verify no compilation errors
- [ ] Test via CLI: `./jtag wall/write --room="general" --doc="test.md" --content="# Test"`
- [ ] Verify WallDocumentEntity created in database
- [ ] Verify file created at correct path
- [ ] Test via CLI: `./jtag wall/read --room="general" --doc="test.md" --toc`
- [ ] Test via CLI: `./jtag wall/list --room="general"`
- [ ] Verify room name → UUID resolution
- [ ] Test AI usage (send message asking AI to use wall/write)
- [ ] Deploy with `npm start` and validate

### Phase 2: Lease System Integration (Future)

**1. Create LeaseDaemon**
- [ ] Daemon initialization and lifecycle
- [ ] FileLease entity CRUD operations
- [ ] Lease acquisition logic (check existing, create new, or queue)
- [ ] Lease expiration monitor (background task every 60s)
- [ ] Queue management (notify next in line)
- [ ] Event emission (lease:acquired, lease:expired, etc.)

**2. Implement lease/request command**
- [ ] Create server command implementation
- [ ] Check file protection level
- [ ] If UNRESTRICTED: grant immediate lease
- [ ] If PEER_REVIEW/SENIOR_REVIEW: create ApprovalRequest
- [ ] If HUMAN_REVIEW: queue for human approval
- [ ] If file already leased: add to LeaseQueue
- [ ] Return lease or queue position

**3. Implement lease/vote-kick command**
- [ ] Create server command implementation
- [ ] Validate kick initiator (must be persona or human)
- [ ] Calculate required votes based on protection level
- [ ] Create KickVote entity
- [ ] Broadcast to all participants
- [ ] Monitor votes until threshold or timeout
- [ ] Execute kick if passed (commit staged edits, revoke lease)

**4. Integrate leases with wall commands**
- [ ] wall/write: Check for active lease before writing
- [ ] wall/write: If no lease, auto-request one (or fail)
- [ ] wall/write: Stage edits in FileLease.stagedEdits
- [ ] wall/write: Actual file write only happens on lease commit
- [ ] Add lease/commit command to finalize changes
- [ ] Add lease/rollback command to abandon changes

---

## Error Handling

### Common Errors

1. **Room not found**
   - Error: `Room "${roomName}" not found`
   - Solution: Query DataDaemon for RoomEntity by name or UUID

2. **Document not found**
   - Error: `Document "${doc}" not found in room "${roomName}"`
   - Solution: Check file exists before reading

3. **Path traversal attempt**
   - Error: `Invalid document name: path traversal not allowed`
   - Solution: Sanitize doc name (remove `..`, leading `/`)

4. **Git commit failure**
   - Error: `Failed to commit: ${gitError}`
   - Solution: Ensure git is initialized, file is tracked

5. **Permission denied**
   - Error: `Permission denied: cannot write to ${filePath}`
   - Solution: Check directory permissions, create if missing

### Error Response Format
```typescript
{
  success: false,
  timestamp: "2025-12-03T...",
  context: {...},
  sessionId: "...",
  error: "Human-readable error message",
  details: {
    code: "ROOM_NOT_FOUND" | "DOC_NOT_FOUND" | "PERMISSION_DENIED",
    filePath: "...",
    roomName: "..."
  }
}
```

---

## Security Considerations

### Phase 1 (No Lease System)

1. **Path Traversal Prevention**
   - Sanitize all document names
   - Remove `..`, `/`, `~` from doc names
   - Enforce `.md` extension

2. **Room Access Control**
   - Only allow writes to rooms user is member of
   - Check RoomEntity.members includes user.id

3. **Content Validation**
   - Validate content is valid UTF-8
   - Check file size limits (e.g., 10MB max)

### Phase 2 (With Lease System)

4. **Lease Expiration**
   - Automatic rollback after 30 minutes (default)
   - Prevents indefinite locks

5. **Democratic Governance**
   - Kick votes require 3+ participants
   - Prevents single AI abuse

6. **Human Override**
   - Joel can always break any lease
   - Emergency access preserved

---

## Performance Considerations

1. **File I/O**
   - Use async file operations (fs.promises)
   - Stream large files instead of loading entirely

2. **Git Operations**
   - Batch commits when possible
   - Async spawn for git commands

3. **Room Resolution**
   - Cache room name → UUID mappings
   - Avoid repeated database queries

4. **TOC Generation**
   - Parse markdown incrementally
   - Cache TOC for frequently accessed docs

---

## Testing Strategy

### Unit Tests
```typescript
// tests/unit/WallManager.test.ts
describe('WallManager', () => {
  test('sanitizeDocName removes path traversal', () => {
    expect(sanitizeDocName('../etc/passwd')).toBe('etcpasswd.md');
  });
  
  test('resolveRoomPath handles UUID', async () => {
    const result = await wallManager.resolveRoomPath('5e71a0c8-...');
    expect(result.roomId).toBe('5e71a0c8-...');
  });
  
  test('generateTOC extracts markdown headers', async () => {
    const content = '## Header 1\n### Sub 1\n## Header 2';
    const toc = await wallManager.generateTOC(content);
    expect(toc).toHaveLength(3);
  });
});
```

### Integration Tests
```typescript
// tests/integration/wall-commands.test.ts
describe('Wall Commands Integration', () => {
  test('wall/write creates file and commits', async () => {
    const result = await Commands.execute('wall/write', {
      room: 'general',
      doc: 'test.md',
      content: '# Test'
    });
    expect(result.success).toBe(true);
    expect(result.commitHash).toBeDefined();
  });
  
  test('wall/read returns TOC', async () => {
    const result = await Commands.execute('wall/read', {
      room: 'general',
      doc: 'test.md',
      toc: true
    });
    expect(result.content).toContain('# Test (line 1)');
  });
});
```

### E2E Tests
```bash
# tests/e2e/wall-ai-collaboration.sh
./jtag wall/write --room="general" --doc="collab-test.md" --content="# AI Collaboration Test"
./jtag collaboration/chat/send --room="general" --message="Please read the wall document collab-test.md and add your thoughts"
sleep 10
./jtag collaboration/chat/export --room="general" --limit=5
./jtag wall/read --room="general" --doc="collab-test.md"
# Expect AI to have appended content
```

---

## Deployment Checklist

- [ ] Run `npm run build:ts` - ensure no compilation errors
- [ ] Run unit tests - all pass
- [ ] Run integration tests - all pass
- [ ] Deploy with `npm start` - wait 130 seconds
- [ ] Test CLI: `./jtag wall/write --room="general" --doc="deployment-test.md" --content="# Test"`
- [ ] Test CLI: `./jtag wall/read --room="general" --doc="deployment-test.md" --toc`
- [ ] Test CLI: `./jtag wall/list --room="general"`
- [ ] Test AI usage: Send chat message asking AI to create a wall document
- [ ] Verify git commits: `cd .continuum/shared/rooms/{uuid} && git log`
- [ ] Document usage in CLAUDE.md

---

## Future Enhancements

1. **Rich Media Support**
   - Images, diagrams, code snippets
   - Embedded visualizations

2. **Version History**
   - wall/history command (git log)
   - wall/diff command (git diff)
   - wall/restore command (git checkout)

3. **Search and Discovery**
   - Full-text search across all room walls
   - Tag-based organization
   - Related document suggestions

4. **Collaborative Editing**
   - Real-time collaborative editing (Operational Transform)
   - Conflict resolution for simultaneous edits
   - Live cursors and presence

5. **Export and Import**
   - Export room walls to PDF/HTML
   - Import external documents
   - Sync with external wikis

---

## References

- [ROOM-WALLS.md](./ROOM-WALLS.md) - Original design document
- [COLLABORATIVE-EDITING-SYSTEM.md](./COLLABORATIVE-EDITING-SYSTEM.md) - Lease system architecture
- [WallTypes.ts](../commands/wall/shared/WallTypes.ts) - Command type definitions
- [LeaseTypes.ts](../shared/LeaseTypes.ts) - Lease entity definitions

---

**End of Implementation Architecture**


---

## Event System Integration

### Wall Document Change Events

When any wall document is modified, broadcast events so all AIs in the room are notified.

**Event Types:**
```typescript
'wall:document:written' - Document created or updated
'wall:document:deleted' - Document removed
'wall:document:renamed' - Document name changed
```

**Event Payload for wall:document:written:**
```typescript
{
  room: string,           // Room name (e.g., "general")
  roomId: UUID,           // Room UUID
  doc: string,            // Document name
  author: string,         // Who made the change
  action: 'created' | 'updated' | 'appended',
  
  // Short summary for notification
  summary: {
    lineCount: number,    // Total lines after change
    byteCount: number,    // Total bytes after change
    linesAdded: number,   // Lines added in this change
    linesRemoved: number, // Lines removed in this change
  },
  
  commitHash: string,     // Git commit hash
  timestamp: string,      // ISO timestamp
  
  // Preview of change (first 3 lines or last 3 lines if append)
  preview?: string
}
```

**Example Notification Message:**
```
🔔 Helper AI updated governance-framework.md in #general
   +15 lines, 847 bytes → 1,124 bytes (3 sections added)
   Preview: "## 3. Emergency Protocols..."
   
   Use: ./jtag wall/read --room="general" --doc="governance-framework.md" --toc
```

### Implementation in WallManager

```typescript
class WallManager {
  async writeDocument(
    room: UUID | string, 
    doc: string, 
    content: string, 
    append?: boolean, 
    author?: string
  ): Promise<WriteResult> {
    // ... write file logic ...
    
    // Calculate change metrics
    const oldLineCount = existingContent ? existingContent.split('\n').length : 0;
    const newLineCount = finalContent.split('\n').length;
    const linesAdded = newLineCount - oldLineCount;
    
    // Broadcast event
    await Events.emit('wall:document:written', {
      room: roomInfo.name,
      roomId: roomInfo.id,
      doc: sanitizedDoc,
      author,
      action: existingContent ? (append ? 'appended' : 'updated') : 'created',
      summary: {
        lineCount: newLineCount,
        byteCount: finalContent.length,
        linesAdded: Math.max(0, linesAdded),
        linesRemoved: Math.max(0, -linesAdded)
      },
      commitHash: commitResult.hash,
      timestamp: new Date().toISOString(),
      preview: this.generatePreview(finalContent, append, linesAdded)
    });
    
    return result;
  }
  
  private generatePreview(content: string, append: boolean, linesAdded: number): string {
    const lines = content.split('\n');
    
    if (append && linesAdded > 0) {
      // Show last 3 lines added
      return lines.slice(-Math.min(3, linesAdded)).join('\n');
    } else {
      // Show first 3 lines of document
      return lines.slice(0, 3).join('\n');
    }
  }
}
```

### AI Notification Handler

AIs can subscribe to wall events and receive compact notifications:

```typescript
// In PersonaUser or chat system
Events.subscribe('wall:document:written', async (event) => {
  // Only notify AIs in the affected room
  if (!this.isInRoom(event.roomId)) return;
  
  // Generate friendly notification
  const notification = this.formatWallNotification(event);
  
  // Post to chat or internal notification system
  await this.handleNotification(notification);
});
```

**Notification Format (Short & Sweet):**
```
🔔 {author} {action} {doc} in #{room}
   {lineChange} lines, {oldSize} → {newSize} bytes
   Preview: "{first line or three}..."
```

**Example Notifications:**
```
🔔 DeepSeek Assistant updated governance-framework.md in #general
   +15 lines, 847 → 1,124 bytes
   Preview: "## 3. Emergency Protocols..."

🔔 Helper AI created dashboard-design.md in #general
   23 lines, 1,456 bytes
   Preview: "# AI Infrastructure Dashboard..."

🔔 Grok appended meeting-notes.md in #general
   +5 lines, 2,103 → 2,458 bytes
   Preview: "- Decision: Adopt token-based governance..."
```

### Why This Matters

1. **Awareness**: AIs know when documents change without polling
2. **Context**: Short summary tells them if they should care
3. **Efficiency**: No need to repeatedly read documents to check for changes
4. **Collaboration**: AIs can coordinate ("I see you updated section 3, I'll work on section 4")
5. **Verification**: AIs can verify their own writes succeeded

### Chat Integration

When an AI receives a wall notification, it can:
- Acknowledge the change in chat: "I see Helper AI updated the governance doc - reviewing now"
- Coordinate work: "DeepSeek is working on section 3, I'll take section 4"
- Verify completeness: "All sections now complete - governance framework is ready for review"

This creates a **natural collaborative flow** where AIs are aware of each other's contributions without constant polling or confusion.


---

## Room-Scoped Event Subscriptions

**Key Insight**: Wall events work exactly like chat messages - you only receive notifications for rooms you're subscribed to.

### Simple Pattern
```typescript
// PersonaUser joins a room
async joinRoom(roomId: UUID) {
  // Subscribe to chat messages in this room
  Events.subscribe(`chat:message:${roomId}`, this.handleChatMessage);
  
  // Subscribe to wall updates in this room
  Events.subscribe(`wall:document:${roomId}`, this.handleWallUpdate);
}

// When document is written
Events.emit(`wall:document:${roomId}`, {
  room: roomName,
  roomId: roomId,
  doc: 'governance.md',
  author: 'Helper AI',
  summary: {...},
  preview: '...'
});
```

### Why Room-Scoped?

1. **Privacy**: AIs only see updates in rooms they're members of
2. **Efficiency**: No filtering needed - subscription = membership
3. **Simplicity**: Same pattern as chat messages
4. **Scalability**: Event system only routes to relevant subscribers

### Implementation

```typescript
// In WallManager.writeDocument()
async writeDocument(...): Promise<WriteResult> {
  // ... write file logic ...
  
  // Emit room-scoped event (same pattern as chat)
  await Events.emit(`wall:document:${roomId}`, {
    room: roomInfo.name,
    roomId: roomInfo.id,
    doc: sanitizedDoc,
    author,
    action: 'updated',
    summary: { lineCount, byteCount, linesAdded, linesRemoved },
    commitHash,
    timestamp: new Date().toISOString(),
    preview
  });
}

// In PersonaUser or chat handler
Events.subscribe(`wall:document:${this.currentRoomId}`, (event) => {
  // Only fires for THIS room
  this.notifyWallUpdate(event);
});
```

### Benefits

- **Just works** - Same pattern as existing chat messages
- **No complexity** - No need to filter events by room
- **Secure** - AIs can't snoop on other rooms' documents
- **Performant** - Event routing is already optimized

