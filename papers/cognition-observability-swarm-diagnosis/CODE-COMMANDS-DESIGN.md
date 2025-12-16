# Code Commands Design - AI Code Interaction

## Vision

Enable AI personas to autonomously read, search, analyze, and eventually execute code through a secure, observable command interface. This transforms personas from passive chat participants into active code collaborators.

## Design Principles

1. **Read-Only First** - Start with safe operations (read, search, list)
2. **Progressive Permission** - Gradually enable write operations with safeguards
3. **Observable** - All operations emit events for cognition tracking
4. **Secure** - Path validation, sandboxing, rate limiting built-in
5. **Collaborative** - Multiple personas can work on same codebase simultaneously

## Phase 1: Read Operations (CURRENT)

### ✅ code/read - Read source files
```bash
./jtag code/read --path="continuum/src/debug/jtag/package.json"
./jtag code/read --path="src/PersonaUser.ts" --startLine=100 --endLine=150
```

**Features:**
- Line range selection
- File metadata (size, lines, modified date)
- Caching with TTL
- Path validation (no directory traversal)
- Event emission: `code:file:read`

**Status:** ✅ Implemented

## Phase 2A: Search & Discovery (NEXT)

### code/search - Search code content
```bash
./jtag code/search --pattern="class.*User" --filePattern="**/*.ts"
./jtag code/search --pattern="TODO" --contextLines=3
./jtag code/search --query="function that handles authentication"
```

**Features:**
- Regex search across codebase
- File pattern filtering (glob)
- Context lines around matches
- Semantic search (future: embeddings)
- Event emission: `code:search`

**Use Cases:**
- Find similar implementations
- Locate bugs by pattern
- Discover API usage examples
- Search TODOs and FIXMEs

### code/list - List files and directories
```bash
./jtag code/list --path="src/debug/jtag/commands" --pattern="*.ts"
./jtag code/list --path="daemons" --recursive=true --type="directory"
```

**Features:**
- Directory traversal with filtering
- Recursive listing
- File type filtering
- Size and modification metadata
- Event emission: `code:list`

**Use Cases:**
- Understand project structure
- Find related files
- Build file trees for context

### code/tree - Generate directory tree
```bash
./jtag code/tree --path="src/debug/jtag" --depth=3 --excludePatterns="node_modules,dist"
```

**Features:**
- ASCII tree visualization
- Depth limiting
- Pattern exclusion
- File counts and sizes
- Event emission: `code:tree`

**Use Cases:**
- Visualize project structure
- Share codebase layout
- Understand module organization

## Phase 2B: Analysis Operations

### code/grep - Fast grep-like search
```bash
./jtag code/grep --pattern="import.*React" --path="src" --fileType="tsx"
./jtag code/grep --pattern="class \w+" --countOnly=true
```

**Features:**
- Ripgrep-powered (fast)
- Line number and context
- File type filtering
- Count mode for statistics
- Event emission: `code:grep`

**Use Cases:**
- Quick symbol lookup
- Import analysis
- Dead code detection

### code/diff - Compare files or versions
```bash
./jtag code/diff --fileA="src/old/User.ts" --fileB="src/new/User.ts"
./jtag code/diff --path="src/User.ts" --gitRef="HEAD~5"
```

**Features:**
- File-to-file comparison
- Git commit comparison
- Unified or side-by-side format
- Line-level highlighting
- Event emission: `code:diff`

**Use Cases:**
- Review changes
- Understand refactoring
- Track API evolution

### code/blame - Git blame annotation
```bash
./jtag code/blame --path="src/PersonaUser.ts" --startLine=100 --endLine=200
```

**Features:**
- Show commit per line
- Author and date info
- Commit message preview
- Event emission: `code:blame`

**Use Cases:**
- Understand code history
- Find who wrote what
- Identify recent changes

### code/log - Git history for files
```bash
./jtag code/log --path="src/PersonaUser.ts" --maxCount=10
./jtag code/log --author="joel" --since="2025-01-01"
```

**Features:**
- Commit history
- Author filtering
- Date range filtering
- Diff inclusion
- Event emission: `code:log`

**Use Cases:**
- Understand evolution
- Find when bugs introduced
- Review recent work

## Phase 3: Static Analysis (FUTURE)

### code/analyze - Static code analysis
```bash
./jtag code/analyze --path="src/PersonaUser.ts" --checks="complexity,dependencies"
./jtag code/analyze --path="src" --recursive=true --format="json"
```

**Features:**
- Cyclomatic complexity
- Dependency graph
- Import analysis
- Dead code detection
- Type coverage
- Event emission: `code:analyze`

**Use Cases:**
- Code quality assessment
- Refactoring candidates
- Technical debt tracking

### code/symbols - Extract symbols/definitions
```bash
./jtag code/symbols --path="src/PersonaUser.ts" --type="class,function"
./jtag code/symbols --query="PersonaUser" --findReferences=true
```

**Features:**
- Class/function extraction
- Interface definitions
- Type signatures
- Reference finding
- Event emission: `code:symbols`

**Use Cases:**
- Generate documentation
- Find usage patterns
- Build symbol index

### code/dependencies - Dependency analysis
```bash
./jtag code/dependencies --path="src/PersonaUser.ts" --depth=2
./jtag code/dependencies --path="." --outdated=true
```

**Features:**
- Import dependency tree
- NPM package analysis
- Circular dependency detection
- Outdated package check
- Event emission: `code:dependencies`

**Use Cases:**
- Understand coupling
- Plan refactoring
- Security audits

## Phase 4: Execution Operations (FUTURE - WITH SANDBOXING)

### code/test - Run tests
```bash
./jtag code/test --path="tests/unit/PersonaUser.test.ts"
./jtag code/test --pattern="PersonaUser" --watch=false
```

**Features:**
- Test execution
- Coverage reporting
- Failure details
- Sandboxed execution
- Event emission: `code:test`

**Use Cases:**
- Verify changes
- Debug test failures
- Generate coverage reports

### code/lint - Run linter
```bash
./jtag code/lint --path="src/PersonaUser.ts" --fix=false
./jtag code/lint --path="src" --rules="strict"
```

**Features:**
- ESLint integration
- Auto-fix mode
- Rule configuration
- Event emission: `code:lint`

**Use Cases:**
- Code quality checks
- Style enforcement
- Pre-commit validation

### code/format - Format code
```bash
./jtag code/format --path="src/PersonaUser.ts" --dryRun=true
./jtag code/format --path="src" --write=true
```

**Features:**
- Prettier integration
- Dry-run mode
- Configuration loading
- Event emission: `code:format`

**Use Cases:**
- Consistent formatting
- Pre-commit hooks
- Automated cleanup

### code/run - Execute code (DANGEROUS - NEEDS SANDBOX)
```bash
./jtag code/run --path="scripts/test.ts" --args="--verbose"
./jtag code/run --command="npm test" --timeout=30000
```

**Features:**
- Sandboxed execution
- Timeout limits
- Output capture
- Resource limits (CPU, memory)
- Event emission: `code:run`

**Security:**
- VM isolation
- File system restrictions
- Network restrictions
- Resource quotas
- Audit logging

**Use Cases:**
- Run scripts
- Test changes
- Build projects

## Phase 5: Write Operations (FUTURE - REQUIRES REVIEW)

### code/write - Create/modify files
```bash
./jtag code/write --path="src/NewFeature.ts" --content="..." --createOnly=true
./jtag code/write --path="src/User.ts" --patch="..." --requireReview=true
```

**Features:**
- File creation
- Patch application
- Mandatory review workflow
- Rollback support
- Event emission: `code:write`

**Security:**
- Review queue
- Approval workflow
- Automatic backup
- Diff preview

**Use Cases:**
- Implement features
- Fix bugs
- Refactor code

### code/edit - Edit file sections
```bash
./jtag code/edit --path="src/User.ts" --startLine=100 --endLine=110 --replacement="..."
```

**Features:**
- Line range editing
- Pattern-based replacement
- Preview mode
- Undo support
- Event emission: `code:edit`

**Use Cases:**
- Targeted fixes
- Refactoring
- Documentation updates

### code/delete - Delete files
```bash
./jtag code/delete --path="src/Deprecated.ts" --requireApproval=true
```

**Features:**
- Soft delete (move to .trash)
- Approval workflow
- Undo support
- Event emission: `code:delete`

**Use Cases:**
- Remove deprecated code
- Clean up experiments
- Delete generated files

## PersonaUser Integration

### Autonomous Code Tasks

Personas can autonomously:

```typescript
// PersonaUser discovers bug through observation
async investigateBug(symptom: string): Promise<void> {
  // 1. Search for error patterns
  const matches = await Commands.execute('code/search', {
    pattern: symptom,
    contextLines: 5
  });

  // 2. Read relevant files
  for (const match of matches.matches.slice(0, 3)) {
    const code = await Commands.execute('code/read', {
      path: match.file,
      startLine: match.line - 20,
      endLine: match.line + 20
    });

    // 3. Analyze context
    await this.analyzeCodeContext(code);
  }

  // 4. Check git history
  const history = await Commands.execute('code/log', {
    path: matches.matches[0].file,
    maxCount: 5
  });

  // 5. Report findings
  await this.reportFindings(matches, history);
}
```

### Collaborative Debugging

Multiple personas working together:

```typescript
// Teacher AI: "I'll check the implementation"
await Commands.execute('code/read', {
  path: 'src/PersonaUser.ts',
  startLine: 100,
  endLine: 200
});

// Code Review AI: "I'll check tests"
await Commands.execute('code/search', {
  pattern: 'PersonaUser.*test',
  filePattern: '**/*.test.ts'
});

// Helper AI: "I'll check recent changes"
await Commands.execute('code/log', {
  path: 'src/PersonaUser.ts',
  maxCount: 10
});
```

### Event-Driven Workflow

```typescript
// Subscribe to code operations
Events.subscribe('code:file:read', async (event) => {
  // Track what files personas are reading
  await CognitionLogger.logStateSnapshot({
    currentFocus: 'code-review',
    workingMemory: [{
      thoughtType: 'observation',
      thoughtContent: `Reading ${event.path}`,
      importance: 0.7
    }]
  });
});

Events.subscribe('code:search', async (event) => {
  // Observe search patterns
  await this.learnSearchPattern(event.pattern);
});
```

## Security Model

### Path Validation
- All paths validated against repository root
- No directory traversal (`../`)
- Whitelist of allowed directories
- Blacklist of sensitive paths (`.env`, credentials)

### Rate Limiting
- Per-persona operation limits
- Cooldown periods for expensive operations
- Burst protection

### Audit Logging
- All operations logged to database
- Who, what, when, where
- Queryable for security review

### Permission Levels

**Level 1: Read-Only (Current)**
- code/read, code/search, code/list, code/tree
- No modifications possible
- Safe for all personas

**Level 2: Analysis (Future)**
- code/analyze, code/symbols, code/dependencies
- Computational but safe
- Requires approval for expensive ops

**Level 3: Execution (Future)**
- code/test, code/lint, code/format
- Sandboxed execution
- Resource limits enforced

**Level 4: Write (Future)**
- code/write, code/edit, code/delete
- Requires human approval
- Audit trail mandatory

## Implementation Roadmap

### Week 1: Phase 2A - Search & Discovery
- [ ] Implement code/search with regex
- [ ] Implement code/list with filtering
- [ ] Implement code/tree visualization
- [ ] Add tests for all commands

### Week 2: Phase 2B - Analysis Operations
- [ ] Implement code/grep (ripgrep integration)
- [ ] Implement code/diff
- [ ] Implement code/blame
- [ ] Enhance code/log with git integration

### Week 3: Phase 3 - Static Analysis
- [ ] Implement code/analyze (complexity, dependencies)
- [ ] Implement code/symbols (AST parsing)
- [ ] Implement code/dependencies (import graph)
- [ ] Generate analysis reports

### Week 4: Phase 4 - Execution (Sandboxed)
- [ ] Design sandbox architecture
- [ ] Implement code/test runner
- [ ] Implement code/lint integration
- [ ] Implement code/format integration

### Future: Phase 5 - Write Operations
- [ ] Design approval workflow
- [ ] Implement code/write with review
- [ ] Implement code/edit with undo
- [ ] Implement code/delete with safety

## Success Metrics

**Usage Metrics:**
- Commands per persona per hour
- Most-used commands
- Average operation latency
- Cache hit rates

**Effectiveness Metrics:**
- Bugs found by personas
- Code reviews completed
- Refactorings suggested
- Tests written

**Collaboration Metrics:**
- Multi-persona debugging sessions
- Shared code discoveries
- Cross-persona insights

**Safety Metrics:**
- Path validation blocks
- Rate limit triggers
- Security audit events
- Failed operations

## Example Use Cases

### Use Case 1: Bug Investigation
```bash
# Persona observes error in logs
./jtag code/search --pattern="Error.*authentication" --contextLines=5

# Read suspect files
./jtag code/read --path="src/auth/AuthService.ts"

# Check recent changes
./jtag code/log --path="src/auth/AuthService.ts" --maxCount=5

# Post findings to chat
./jtag collaboration/chat/send --room="general" --message="Found issue in AuthService.ts:142..."
```

### Use Case 2: Code Review
```bash
# List changed files
./jtag code/log --since="yesterday" --nameOnly=true

# Review each file
./jtag code/diff --path="src/User.ts" --gitRef="HEAD~1"

# Check complexity
./jtag code/analyze --path="src/User.ts" --checks="complexity"

# Post review comments
./jtag collaboration/chat/send --room="code-review" --message="Reviewed 5 files..."
```

### Use Case 3: Documentation Generation
```bash
# Extract symbols
./jtag code/symbols --path="src/PersonaUser.ts" --type="class,function"

# Read implementations
./jtag code/read --path="src/PersonaUser.ts"

# Generate docs
# (Persona processes and writes documentation)
```

## Next Steps

1. **Implement code/search** - Most requested by personas
2. **Add code/list and code/tree** - Essential for discovery
3. **Integrate with PersonaUser** - Add helper methods
4. **Create examples** - Show personas how to use commands
5. **Monitor usage** - Track which commands personas use most
6. **Iterate** - Add commands based on actual usage patterns

---

**Status:** Phase 1 complete (code/read working), Phase 2A ready to implement
**Owner:** CodeDaemon team
**Last Updated:** 2025-11-17
