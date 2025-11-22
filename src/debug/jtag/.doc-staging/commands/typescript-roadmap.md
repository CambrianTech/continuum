# TypeScript Commands Roadmap

**Vision**: Comprehensive TypeScript tooling through JTAG commands, enabling type reflection, compilation, validation, analysis, and transformation. Foundation for "hot code editing like we do CSS."

---

## Phase 1: Foundation ✅ COMPLETE

**Built**: `system/typescript/shared/TypeScriptCompiler.ts`

**Capabilities**:
- Wraps TypeScript compiler API with proper module resolution
- Loads tsconfig.json and creates ts.Program with all source files
- **Key method**: `getInterfaceInfo()` - resolves ALL properties including inherited ones across files
- `findInterfaces()` - Pattern-based interface discovery
- `compile()` - Full TypeScript compilation
- Exposes `getTypeChecker()` and `getProgram()` for advanced operations

**Why this matters**:
- Single source of truth for TypeScript operations
- Properly resolves cross-file inheritance (the help command problem)
- Foundation for all future TypeScript-based commands

---

## Phase 2: Commands (Using the Foundation)

### ✅ 1. schema/generate (COMPLETE)

**Status**: Implemented and deployed

**Purpose**: Generate JSON schemas from TypeScript interfaces with proper cross-file inheritance

**Usage**:
```bash
# Generate schema for specific interface
./jtag schema/generate --interface="DataReadParams" \
  --file="commands/data/read/shared/DataReadTypes.ts"

# Generate schemas matching pattern
./jtag schema/generate --pattern="*Params" --output="schemas.json"
```

**Features**:
- Resolves inheritance across files (BaseDataParams → DataReadParams)
- Extracts all properties including from JTAGPayload grandparent
- Filters out internal parameters (context, sessionId, backend)
- Used by build script to generate command-schemas.json for help command

**Impact**: Help command now shows complete parameter signatures!

---

### 📋 2. code/reflect (PLANNED)

**Purpose**: Extract type information from any file or interface at runtime

**Planned Usage**:
```bash
# Reflect on specific interface
./jtag code/reflect --interface="UserEntity" \
  --file="system/data/entities/UserEntity.ts"

# Find all interfaces in a directory
./jtag code/reflect --pattern="*Entity" --dir="system/data/entities"

# Get method signatures
./jtag code/reflect --class="DataDaemon" --methods

# Export to JSON for AI consumption
./jtag code/reflect --interface="CommandParams" --output="reflection.json"
```

**Use Cases**:
- AI agents understanding type structure before using APIs
- Dynamic documentation generation
- Code generation based on existing types
- Type-driven UI generation

**Implementation Notes**:
- Use `TypeScriptCompiler.getInterfaceInfo()` for interfaces
- Add `getClassInfo()` method to TypeScriptCompiler for classes
- Add `getMethodSignatures()` for functions/methods
- Return structured JSON with types, inheritance, JSDoc comments

---

### 📋 3. code/compile (PLANNED)

**Purpose**: Compile TypeScript with full control and detailed diagnostics

**Planned Usage**:
```bash
# Compile specific file
./jtag code/compile --file="widgets/chat/chat-widget/ChatWidget.ts"

# Compile with custom options
./jtag code/compile --file="test.ts" \
  --target="ES2020" --module="ESNext" --strict=true

# Check compilation without emitting
./jtag code/compile --file="test.ts" --noEmit

# Get detailed diagnostics
./jtag code/compile --file="test.ts" --diagnostics="verbose"
```

**Use Cases**:
- Pre-deployment type checking
- Custom build configurations
- Type error diagnosis
- CI/CD integration

**Implementation Notes**:
- Use `TypeScriptCompiler.compile()` as base
- Add options for custom compiler settings
- Return structured diagnostic information
- Support for incremental compilation

---

### 📋 4. code/validate (PLANNED)

**Purpose**: Runtime type checking and validation

**Planned Usage**:
```bash
# Validate object matches interface
./jtag code/validate --interface="UserEntity" \
  --data='{"id":"123","displayName":"Test"}' \
  --file="system/data/entities/UserEntity.ts"

# Validate function parameters
./jtag code/validate --function="createUser" \
  --params='[{"displayName":"Test"}]' \
  --file="system/user/shared/UserFactory.ts"

# Generate runtime validators
./jtag code/validate --interface="CommandParams" \
  --generate-validator --output="validators.ts"
```

**Use Cases**:
- API input validation
- Data migration safety checks
- Runtime type assertions
- Test data validation

**Implementation Notes**:
- Use TypeChecker to extract type constraints
- Generate runtime validation functions
- Support for custom validators
- Integration with JSON Schema validation

---

### 📋 5. code/analyze (PLANNED)

**Purpose**: Static analysis and code quality checks

**Planned Usage**:
```bash
# Find unused exports
./jtag code/analyze --type="unused-exports" --dir="commands"

# Find circular dependencies
./jtag code/analyze --type="circular-deps" --file="system/core/JTAGTypes.ts"

# Complexity analysis
./jtag code/analyze --type="complexity" \
  --file="system/user/server/PersonaUser.ts" --threshold=10

# Find type errors without compiling
./jtag code/analyze --type="type-check" --file="test.ts"
```

**Use Cases**:
- Code review automation
- Refactoring guidance
- Dependency graph analysis
- Dead code elimination

**Implementation Notes**:
- Use TypeScript's language service for analysis
- Implement custom visitors for specific checks
- Return actionable recommendations
- Integration with linting tools

---

### 📋 6. code/transform (PLANNED)

**Purpose**: AST-based code transformations

**Planned Usage**:
```bash
# Rename interface across files
./jtag code/transform --type="rename" \
  --interface="OldName" --new-name="NewName" \
  --file="system/types.ts"

# Add JSDoc comments from interface
./jtag code/transform --type="add-jsdoc" \
  --interface="CommandParams" --file="commands/help/shared/HelpTypes.ts"

# Convert interface to type
./jtag code/transform --type="interface-to-type" \
  --interface="UserData" --file="types.ts"

# Extract interface from class
./jtag code/transform --type="extract-interface" \
  --class="DataDaemon" --output="IDataDaemon.ts"
```

**Use Cases**:
- Automated refactoring
- Code generation
- Type definition updates
- Migration scripts

**Implementation Notes**:
- Use TypeScript transformation API
- Implement custom transformers for common operations
- Dry-run mode for safety
- Backup original files before transformation

---

## Phase 3: Integration & Automation

### Hot Code Editing (Future Vision)

**Goal**: Edit TypeScript code with instant feedback like hot CSS injection

**Approach**:
1. Use `code/compile --noEmit` for instant type checking
2. Use `code/validate` for runtime safety
3. Use `code/transform` for refactoring assistance
4. Live reload mechanism similar to CSS hot-injection

**Example Workflow**:
```bash
# 1. Edit TypeScript file
vim widgets/chat/ChatWidget.ts

# 2. Instant type check (no emit)
./jtag code/compile --file="widgets/chat/ChatWidget.ts" --noEmit

# 3. If valid, hot-reload (future mechanism)
./jtag hot-reload --file="widgets/chat/ChatWidget.ts"
```

### Build Pipeline Integration

**Goal**: Integrate TypeScript commands into build process

```bash
# In package.json scripts:
{
  "prebuild": "npx tsx -e 'import { Commands } from \"system/core/shared/Commands\"; Commands.execute(\"code/analyze\", { type: \"type-check\", dir: \".\" })'",
  "generate-schemas": "npx tsx -e 'import { Commands } from \"system/core/shared/Commands\"; Commands.execute(\"schema/generate\", { pattern: \"*Params\", output: \"generated/schemas.json\" })'"
}
```

### AI Agent Integration

**Goal**: Enable AI agents to understand and manipulate TypeScript code

**Capabilities**:
1. AI uses `code/reflect` to understand API signatures
2. AI uses `code/validate` to check generated code
3. AI uses `code/analyze` to identify issues
4. AI uses `code/transform` to apply fixes

**Example AI Workflow**:
```typescript
// AI wants to call a function but doesn't know signature
const signature = await Commands.execute('code/reflect', {
  function: 'createUser',
  file: 'system/user/shared/UserFactory.ts'
});

// AI generates parameters based on signature
const params = generateParams(signature);

// AI validates before executing
const validation = await Commands.execute('code/validate', {
  function: 'createUser',
  params: JSON.stringify(params),
  file: 'system/user/shared/UserFactory.ts'
});

if (validation.valid) {
  // Execute safely
  await createUser(params);
}
```

---

## Implementation Priority

### P0 - Critical (Next Sprint)
1. ✅ schema/generate (COMPLETE)
2. 📋 code/reflect - Essential for AI understanding of types

### P1 - High Value
3. 📋 code/compile - Needed for hot code editing
4. 📋 code/validate - Safety for dynamic code generation

### P2 - Quality of Life
5. 📋 code/analyze - Code quality and refactoring
6. 📋 code/transform - Advanced refactoring automation

---

## Success Metrics

1. **Help Command** ✅ - Shows complete parameter signatures with inheritance
2. **AI Type Understanding** - AIs can query type info via code/reflect
3. **Hot Code Editing** - Edit TypeScript with <2s feedback loop
4. **Zero Type Errors** - Build pipeline catches all type issues pre-commit
5. **AI Code Generation** - AIs generate valid TypeScript using code/validate

---

## Dependencies

- TypeScript 5.x compiler API
- ts.Program with proper module resolution
- Access to tsconfig.json
- File system access for reading source files

---

## Related Documentation

- [TypeScriptCompiler API](./shared/TypeScriptCompiler.ts) - Foundation implementation
- [Command Architecture](../../docs/ARCHITECTURE-RULES.md) - Command patterns
- [Universal Primitives](../../docs/UNIVERSAL-PRIMITIVES.md) - Commands.execute()

---

## Notes

**Why not use existing tools?**
- `tsc` - No programmatic API for reflection
- `ts-node` - Runtime only, no introspection
- `typescript` npm package - We're building on this, but wrapping for JTAG

**Key Insight from Help Command Fix**:
The original problem (help showing incomplete parameters) revealed that proper TypeScript compiler integration is essential for any system that needs to understand its own types. This roadmap extends that insight to full TypeScript tooling.

**Future Vision**:
When PersonaUser AIs can use `code/reflect` to understand types, `code/validate` to check their work, and `code/transform` to refactor code, they'll be able to contribute to the codebase alongside human developers. This is the path to truly autonomous AI development.
