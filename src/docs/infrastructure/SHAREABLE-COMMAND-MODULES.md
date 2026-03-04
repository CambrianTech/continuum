# Shareable Command Modules

**Status**: Design Complete, Ready for Implementation
**Target**: Phase 3b Generator Enhancement
**Priority**: Foundation for decentralized command ecosystem

## Vision

Transform JTAG commands into **first-class npm packages** that can be:
- Developed independently with own dependencies
- Tested locally via `npm test`
- Packaged with `npm pack` → `.tgz` file
- Shared via any transport (npmjs, mesh, USB, Git)
- Installed with standard npm tooling
- Distributed in decentralized mesh (future)

## The Key Insight

**npm format is the interface, not the implementation.**

We use npm's conventions (package.json, semver, scopes) but are NOT locked into npmjs.com infrastructure. This enables:
- ✅ Centralized distribution (npmjs.com) - convenience
- ✅ Private registries (companies) - control
- ✅ Direct sharing (.tgz files) - works today
- ✅ Decentralized mesh (future) - resilience
- ✅ Air-gapped environments - security

## Architecture

### Command Module Structure

```
commands/hello/
├── package.json           # Self-contained npm package
├── README.md              # Command documentation
├── .npmignore             # Exclude tests from published package
├── shared/
│   └── HelloTypes.ts      # Shared type definitions
├── browser/
│   └── HelloBrowser.ts    # Browser-specific implementation
├── server/
│   └── HelloServer.ts     # Server-specific implementation
└── test/
    ├── unit/
    │   └── Hello.test.ts  # Unit tests (mocks OK)
    └── integration/
        └── HelloIntegration.test.ts  # Live system tests
```

### Generated package.json

```json
{
  "name": "@jtag-commands/hello",
  "version": "1.0.0",
  "description": "Hello world command for JTAG",
  "main": "server/HelloServer.ts",
  "types": "shared/HelloTypes.ts",

  "scripts": {
    "test": "npm run test:unit && npm run test:integration",
    "test:unit": "npx vitest run test/unit/*.test.ts",
    "test:integration": "npx tsx test/integration/HelloIntegration.test.ts",
    "lint": "npx eslint **/*.ts",
    "typecheck": "npx tsc --noEmit"
  },

  "dependencies": {
    // Command-specific deps (e.g., "sharp": "^0.33.0" for media commands)
  },

  "peerDependencies": {
    "@jtag/core": "*"
  },

  "files": [
    "shared/**/*.ts",
    "browser/**/*.ts",
    "server/**/*.ts",
    "README.md"
  ],

  "keywords": ["jtag", "command", "hello"],
  "license": "MIT",
  "author": "JTAG Developers",
  "repository": {
    "type": "git",
    "url": "https://github.com/your-org/jtag-commands"
  }
}
```

### Command Group Structure

For multi-command modules like `data/*`:

```
commands/data/
├── package.json              # Group-level package
├── README.md                 # All data commands documented
├── list/
│   ├── shared/DataListTypes.ts
│   ├── server/DataListServer.ts
│   └── test/...
├── create/
│   ├── shared/DataCreateTypes.ts
│   ├── server/DataCreateServer.ts
│   └── test/...
├── update/
│   └── ...
└── delete/
    └── ...
```

**Group package.json**:

```json
{
  "name": "@jtag-commands/data",
  "version": "2.0.0",
  "description": "Data CRUD operations for JTAG",

  "scripts": {
    "test": "npm run test:list && npm run test:create && npm run test:update && npm run test:delete",
    "test:list": "npx tsx list/test/integration/DataListIntegration.test.ts",
    "test:create": "npx tsx create/test/integration/DataCreateIntegration.test.ts",
    "test:update": "npx tsx update/test/integration/DataUpdateIntegration.test.ts",
    "test:delete": "npx tsx delete/test/integration/DataDeleteIntegration.test.ts",
    "test:all": "find . -name '*Integration.test.ts' -exec npx tsx {} \\;",
    "lint": "npx eslint **/*.ts"
  },

  "files": [
    "*/shared/**/*.ts",
    "*/browser/**/*.ts",
    "*/server/**/*.ts"
  ]
}
```

## Distribution Methods

### 1. Direct Sharing (Works Today)

```bash
# Developer packages command
cd commands/hello
npm pack
# Output: jtag-commands-hello-1.0.0.tgz

# Share via any method
scp jtag-commands-hello-1.0.0.tgz colleague@server:/tmp/
# Or: Slack, email, USB drive, Git LFS, etc.

# Colleague installs
npm install /tmp/jtag-commands-hello-1.0.0.tgz
# Works offline! All dependencies included.
```

### 2. npmjs.com (Convenience)

```bash
# Publish to public registry
npm publish

# Anyone installs
npm install @jtag-commands/hello
```

### 3. Private Registry (Companies)

```bash
# Configure private registry
npm config set @acme-corp:registry https://npm.acme-corp.com

# Publish internally
npm publish

# Install company commands
npm install @acme-corp/internal-api
```

### 4. Decentralized Mesh (Future)

```bash
# Announce command to mesh
jtag mesh host @jtag-commands/vector-search

# Discover commands
jtag mesh discover "vector-search"

# Install from fastest peer
jtag mesh install @jtag-commands/vector-search --auto-peer

# Verify integrity
jtag mesh verify @jtag-commands/vector-search
```

## Root Package.json Integration

### Test Orchestration

```json
{
  "scripts": {
    "test:command": "cd commands/$npm_config_cmd && npm test",
    "test:commands": "find commands -maxdepth 2 -name package.json -execdir npm test \\;",
    "lint:commands": "find commands -maxdepth 2 -name package.json -execdir npm run lint \\;",
    "pack:command": "cd commands/$npm_config_cmd && npm pack"
  }
}
```

**Usage**:

```bash
# Test single command locally
cd commands/hello && npm test

# Test from root
npm run test:command --cmd=hello

# Test ALL commands
npm run test:commands

# Package for sharing
npm run pack:command --cmd=data
# Creates: commands/data/jtag-commands-data-2.0.0.tgz
```

## Benefits

### 1. Self-Contained
- Own dependencies, tests, docs in one directory
- Works standalone: `cd commands/hello && npm test`
- No reliance on root package.json

### 2. Shareable
- `npm pack` creates `.tgz` archive
- Send to colleague, they extract and use
- Could publish to npm registry later

### 3. Testable
- Unit tests: `npm run test:unit`
- Integration tests: `npm run test:integration`
- Both: `npm test`

### 4. Versionable
- Semantic versioning per command/group
- Breaking changes isolated to one module
- `package-lock.json` ensures reproducible installs

### 5. Discoverable
- Dynamic file system scanning still works
- No hard-coded imports needed
- Drop in `commands/` and it's auto-registered

### 6. Dependency Management
- npm handles version conflicts automatically
- Deduplicates shared dependencies
- Native bindings work across platforms

### 7. Censorship-Resistant
- No central point of failure
- Can be distributed via any method
- Works offline/air-gapped
- Cryptographic verification possible

## Retrofitting Existing Commands

### Non-Destructive Strategy

The generator must be able to bring **existing commands** into compliance without breaking them:

```bash
# Retrofit existing command with package.json
./jtag generate data/list --retrofit=true

# Or retrofit all commands
./jtag generate --retrofit-all=true
```

**What this does**:
1. **Check existing structure** - detects what files are present
2. **Add missing files only** - doesn't overwrite existing code
3. **Generate package.json** - based on discovered structure
4. **Infer metadata** - extracts description from existing README/types
5. **Preserve customizations** - works around existing patterns

### Retrofit Algorithm

```typescript
async retrofitCommand(commandPath: string): Promise<void> {
  const commandName = path.basename(commandPath);

  // Step 1: Scan existing structure
  const structure = await this.scanCommandStructure(commandPath);

  // Step 2: Check if package.json exists
  if (structure.hasPackageJson) {
    console.log(`⏭️  Skipping ${commandName} - package.json exists`);
    return;
  }

  // Step 3: Infer metadata from existing files
  const metadata = await this.inferMetadata(structure);

  // Step 4: Generate package.json (non-destructive)
  await this.generatePackageJson(commandPath, metadata);

  // Step 5: Generate .npmignore if missing
  if (!structure.hasNpmignore) {
    await this.generateNpmignore(commandPath);
  }

  // Step 6: Add test scripts if tests exist
  if (structure.hasTests) {
    console.log(`✅ Added test scripts for ${commandName}`);
  }

  console.log(`✅ Retrofitted ${commandName}`);
}

async inferMetadata(structure: CommandStructure): Promise<CommandMetadata> {
  return {
    name: structure.name,
    description: structure.readme?.description || `${structure.name} command`,
    version: '1.0.0',
    hasTests: structure.hasTests,
    hasBrowser: structure.hasBrowser,
    hasServer: structure.hasServer,
    dependencies: this.detectDependencies(structure)
  };
}
```

### Safe Package.json Generation

**For existing commands, infer everything**:

```json
{
  "name": "@jtag-commands/{{INFERRED_NAME}}",
  "version": "1.0.0",
  "description": "{{INFERRED_FROM_README_OR_TYPES}}",
  "main": "{{DETECTED_MAIN_FILE}}",

  "scripts": {
    "test": "{{DETECTED_TEST_COMMAND}}",
    "lint": "npx eslint **/*.ts"
  },

  "dependencies": {
    // Inferred from imports in TypeScript files
  },

  "files": [
    // Include all existing directories
    "{{DETECTED_DIRS}}"
  ]
}
```

### Detection Logic

```typescript
async scanCommandStructure(commandPath: string): Promise<CommandStructure> {
  const structure: CommandStructure = {
    name: path.basename(commandPath),
    hasPackageJson: await fs.exists(path.join(commandPath, 'package.json')),
    hasNpmignore: await fs.exists(path.join(commandPath, '.npmignore')),
    hasReadme: await fs.exists(path.join(commandPath, 'README.md')),
    hasTests: false,
    hasBrowser: false,
    hasServer: false,
    testFiles: [],
    sourceFiles: []
  };

  // Detect test directory
  const testDir = path.join(commandPath, 'test');
  if (await fs.exists(testDir)) {
    structure.hasTests = true;
    structure.testFiles = await this.findFiles(testDir, '*.test.ts');
  }

  // Detect browser implementation
  const browserDir = path.join(commandPath, 'browser');
  structure.hasBrowser = await fs.exists(browserDir);

  // Detect server implementation
  const serverDir = path.join(commandPath, 'server');
  structure.hasServer = await fs.exists(serverDir);

  // Scan all TypeScript files for imports (detect dependencies)
  const allFiles = await this.findFiles(commandPath, '*.ts');
  structure.sourceFiles = allFiles;
  structure.imports = await this.extractImports(allFiles);

  return structure;
}

async extractImports(files: string[]): Promise<string[]> {
  const imports = new Set<string>();

  for (const file of files) {
    const content = await fs.readFile(file, 'utf8');
    // Match: import ... from 'package-name'
    const matches = content.matchAll(/from ['"]([^'"\.][^'"]*)['"]/g);
    for (const match of matches) {
      const pkg = match[1].split('/')[0]; // Get root package name
      if (!pkg.startsWith('@jtag')) { // External deps only
        imports.add(pkg);
      }
    }
  }

  return Array.from(imports);
}
```

### Batch Retrofit

```bash
# Retrofit all commands at once
./jtag generate --retrofit-all=true

# Output:
# 📦 Scanning 147 commands...
# ✅ Retrofitted data/list (inferred 3 dependencies)
# ✅ Retrofitted data/create (inferred 2 dependencies)
# ⏭️  Skipping ping (package.json exists)
# ✅ Retrofitted screenshot (inferred sharp, puppeteer)
# ...
# 🎉 Retrofitted 134 commands, skipped 13 existing
```

### Upgrade Path

**For commands with package.json, offer upgrades**:

```bash
./jtag generate data/list --upgrade=true

# Prompts:
# Found package.json v1.0.0
# Available upgrades:
#   - Add test:integration script
#   - Add lint script
#   - Update dependencies
#   - Add .npmignore
# Apply all? (y/n)
```

### Compatibility

**Generated files work alongside existing structure**:

```
commands/data/list/
├── package.json              # NEW - Generated
├── .npmignore                # NEW - Generated
├── README.md                 # EXISTING - Preserved
├── shared/DataListTypes.ts   # EXISTING - Preserved
├── browser/DataListBrowser.ts # EXISTING - Preserved
├── server/DataListServer.ts  # EXISTING - Preserved
└── test/                     # EXISTING - Preserved
    ├── unit/DataList.test.ts
    └── integration/DataListIntegration.test.ts
```

**No files are overwritten** - only missing files are added.

### Migration Timeline

1. **Generate new commands** - get package.json by default
2. **Retrofit critical commands** - data/*, ai/*, chat/*
3. **Batch retrofit rest** - `--retrofit-all`
4. **Upgrade iteratively** - improve package.json over time

## Implementation Plan

### Phase 1: Generator Enhancement

**Add `package.json.template`**:

```typescript
// generator/templates/command/package.json.template
{
  "name": "@jtag-commands/{{COMMAND_NAME}}",
  "version": "1.0.0",
  "description": "{{DESCRIPTION}}",
  "main": "server/{{CLASS_NAME}}Server.ts",
  "types": "shared/{{CLASS_NAME}}Types.ts",
  "scripts": {
    "test": "npm run test:unit && npm run test:integration",
    "test:unit": "npx vitest run test/unit/*.test.ts",
    "test:integration": "npx tsx test/integration/{{CLASS_NAME}}Integration.test.ts",
    "lint": "npx eslint **/*.ts",
    "typecheck": "npx tsc --noEmit"
  },
  "peerDependencies": {
    "@jtag/core": "*"
  },
  "files": [
    "shared/**/*.ts",
    "browser/**/*.ts",
    "server/**/*.ts",
    "README.md"
  ],
  "keywords": ["jtag", "command", "{{COMMAND_NAME}}"],
  "license": "MIT"
}
```

**Add `.npmignore.template`**:

```
test/
*.test.ts
*.spec.ts
coverage/
.DS_Store
```

**Update CommandGenerator.ts**:

```typescript
async generateCommand(spec: CommandSpec): Promise<void> {
  // Existing generation...

  // NEW: Generate package.json
  const packageJson = await this.templateLoader.load('package.json.template');
  const packageJsonContent = this.tokenBuilder.replace(packageJson, {
    COMMAND_NAME: spec.name,
    CLASS_NAME: this.toClassName(spec.name),
    DESCRIPTION: spec.description
  });
  await fs.writeFile(
    path.join(commandDir, 'package.json'),
    packageJsonContent
  );

  // NEW: Generate .npmignore
  const npmignore = await this.templateLoader.load('.npmignore.template');
  await fs.writeFile(
    path.join(commandDir, '.npmignore'),
    npmignore
  );
}
```

### Phase 2: Root Scripts

**Update root package.json**:

```json
{
  "scripts": {
    "test:command": "cd commands/$npm_config_cmd && npm test",
    "test:commands": "find commands -maxdepth 2 -name package.json -execdir npm test \\;",
    "pack:command": "cd commands/$npm_config_cmd && npm pack",
    "install:command": "npm install commands/$npm_config_cmd/*.tgz"
  }
}
```

### Phase 3: Testing & Validation

1. **Generate test command**:
   ```bash
   ./jtag generate hello
   ```

2. **Verify package.json created**:
   ```bash
   cat commands/hello/package.json
   ```

3. **Test locally**:
   ```bash
   cd commands/hello
   npm test
   ```

4. **Package**:
   ```bash
   npm pack
   # Creates: jtag-commands-hello-1.0.0.tgz
   ```

5. **Test in fresh JTAG instance**:
   ```bash
   cd /tmp/test-jtag
   npm install ~/jtag-commands-hello-1.0.0.tgz
   ./jtag hello
   ```

### Phase 4: Documentation

1. Update CLAUDE.md with command module workflow
2. Document packaging and sharing process
3. Add examples to README

## Future: Decentralized Mesh

### Discovery Protocol

**Broadcast availability**:
```typescript
await jtag.mesh.announce({
  package: '@jtag-commands/vector-search',
  version: '2.1.0',
  hash: 'sha256:abc123...',
  signature: 'ed25519:...',
  capabilities: ['gpu-acceleration', 'batch-processing']
});
```

**Discover commands**:
```typescript
const available = await jtag.mesh.discover({
  query: 'vector-search',
  minVersion: '2.0.0',
  capabilities: ['gpu-acceleration']
});
```

### Cryptographic Verification

**Sign package**:
```bash
jtag sign @jtag-commands/hello --key ~/.jtag/private.key
# Adds signature to package.json
```

**Verify authenticity**:
```bash
jtag verify @jtag-commands/hello --pubkey ~/.jtag/joel.pub
# Checks signature matches
```

### Content Addressing

**Install by hash** (immutable):
```bash
jtag install sha256:a3b2c1d4e5f6...
# Name is metadata, hash is truth
```

### P2P Distribution

**Host commands for mesh**:
```bash
jtag mesh host @jtag-commands/*
# Serve all your commands to peers
```

**Install from mesh**:
```bash
jtag mesh install @jtag-commands/vector-search
# Automatically finds fastest peer
```

## Security Considerations

### Trust Model

1. **Package Signatures**: Author signs with private key
2. **Content Hashing**: SHA-256 of entire package
3. **Trust Web**: Publish public keys, verify signatures
4. **Reproducible Builds**: Lock files ensure exact versions

### Air-Gapped Environments

Commands work completely offline:
1. Bundle all dependencies in `.tgz`
2. Transfer via USB/secure network
3. Install without internet
4. Verify hash matches

### Threat Protection

- **Dependency confusion**: Scoped packages (@jtag-commands/*)
- **Supply chain**: Content addressing, signatures
- **Censorship**: Decentralized mesh, direct sharing
- **Tampering**: Cryptographic hashes, signatures

## Why This Matters

### Resilience Against Centralized Control

**Problem**: npmjs.com could be pressured to remove packages, GitHub could take down repos, DNS could be seized.

**Solution**: Commands are self-contained and can be distributed via:
- Direct `.tgz` sharing (USB, email, Slack)
- Git repositories (decentralized)
- P2P mesh network (no central server)
- IPFS (content-addressed, permanent)

**Result**: Can't shut down what has no center.

### Real-World Precedents

This architecture mirrors successful decentralized systems:
- **BitTorrent**: Distributed file sharing
- **Bitcoin**: Distributed ledger
- **Git**: Distributed version control
- **IPFS**: Distributed storage
- **Tor**: Distributed network

## Timeline

**Phase 1** (This PR): Generate package.json per command
**Phase 2** (Next PR): Root orchestration scripts
**Phase 3** (Q1 2026): Testing and validation workflow
**Phase 4** (Q2 2026): Mesh discovery protocol
**Phase 5** (Q3 2026): Cryptographic signing/verification
**Phase 6** (Q4 2026): P2P distribution network

## Success Criteria

- ✅ Commands have own package.json
- ✅ `npm test` works locally in command dir
- ✅ `npm pack` creates valid .tgz
- ✅ .tgz can be installed elsewhere
- ✅ All dependencies included in package
- ✅ Dynamic discovery still works
- ✅ Zero hard-coded imports needed

## Related Documents

- `UNIVERSAL-PRIMITIVES.md` - Command execution architecture
- `ARCHITECTURE-RULES.md` - Module separation rules
- `CLAUDE.md` - Development workflow

## Conclusion

By making commands **self-contained npm packages**, we create a **censorship-resistant, decentralized command ecosystem** that:
- Works with standard npm tooling today
- Can be distributed via any method
- Has no central point of failure
- Protects against takedowns/censorship
- Enables offline/air-gapped usage
- Supports future mesh distribution

**The format is standard, the distribution is decentralized.**
