# Decluttering Protocol - "Nothing Goes in Root"

## 🎯 **Core Principle: Clean Repository Root**

> **"Nothing ever goes in the root of this repo"** - Architectural Law

The repository root should contain only essential infrastructure files and the `src/` directory with all implementation code.

## 📊 **Current Root Directory Audit**

### **✅ ALLOWED IN ROOT (Essential Infrastructure)**
```
src/                    # All source code
middle-out/            # Architecture documentation
package.json           # NPM configuration
package-lock.json      # Dependency lock file
tsconfig*.json         # TypeScript configuration  
jest.config.*          # Test configuration
eslint.config.js       # Linting configuration
.husky/               # Git hooks
.continuum/           # Runtime session data
.gitignore            # Git configuration
README.md             # Project overview (keep minimal)
continuum             # Main executable script
```

### **❌ IMMEDIATE ARCHIVAL CANDIDATES (52 files/directories)**

#### **Documentation Pollution (24 files)**
```bash
# Legacy documentation files that should be in middle-out/ or src/
ACADEMY_ARCHITECTURE.md          → middle-out/academy/
ACADEMY_SYSTEM_COMPLETE.md       → middle-out/academy/
ADVERSARIAL_ROADMAP.md           → middle-out/research/
AI-POWERED-DEVELOPMENT.md        → middle-out/development/
ARCHITECTURE.md                  → middle-out/architecture/
BROWSER_LOGS_COMPLETE_FIX.md     → middle-out/jtag/
CHECKIN_SUMMARY.md               → archive/legacy-docs/
CLAUDE.md                        → Keep (active bootloader)
CLEANUP_PLAN.md                  → archive/legacy-docs/
CONTINUUM_MANIFESTO.md           → middle-out/research/
DEVTOOLS_AUTO_LAUNCH_MECHANISM.md → middle-out/jtag/
DEVTOOLS_INTEGRATION_PLAN.md     → middle-out/jtag/
DOCUMENTATION_INDEX.md           → archive/legacy-docs/
FILES.md                         → archive/legacy-docs/
IMPLEMENTATION_ROADMAP.md        → middle-out/development/
INTEGRATION_STATUS.md            → archive/legacy-docs/
LANGUAGE_MIGRATION.md            → archive/legacy-docs/
MESH_CHARTER.md                  → middle-out/research/
MIDDLE_OUT_SUCCESS.md            → middle-out/development/
MIGRATION_SYSTEM_COMPLETE.md     → archive/legacy-docs/
MISSION.md                       → middle-out/research/
PERSONA-LORA-INTEGRATION.md      → middle-out/academy/
README-*.md                      → archive/legacy-docs/
RESTORATION-STRATEGY.md          → archive/legacy-docs/
ROADMAP.md                       → middle-out/development/
SCREENSHOT_REFERENCE.md          → middle-out/jtag/
TECHNICAL_DEBT.md                → middle-out/development/ (DONE)
VALIDATION_COMPLETE.md           → archive/legacy-docs/
VISION.md                        → middle-out/research/
WORKING_NOTES.md                 → archive/legacy-docs/
```

#### **Legacy Code Directories (8 directories)**
```bash
# Old development directories that should be archived
academy-sdk/              → archive/legacy-code/
agent-scripts/            → archive/legacy-code/
agents/                   → archive/legacy-code/
archived/                 → archive/legacy-code/ (already partially archived)
examples/                 → src/examples/ or archive/
python-client/            → src/integrations/python/ or archive/
temp-disabled/            → archive/legacy-code/
verification/             → archive/test-artifacts/
```

#### **Build Artifacts & Test Debris (15+ files)**
```bash
# Build artifacts that should be .gitignored
build.log                 → .gitignore
continuum-*.log           → .gitignore
daemon*.log               → .gitignore
server*.log               → .gitignore
startup*.log              → .gitignore
system*.log               → .gitignore
build/                    → .gitignore
coverage/                 → .gitignore
dist/                     → .gitignore

# Test debris
test-*                    → archive/test-debris/
debug-*                   → archive/test-debris/
junk-*/                   → archive/legacy-code/
quick_commit_check_legacy.py → archive/legacy-code/
```

#### **Configuration & Template Pollution (5 directories)**
```bash
# Configuration files that should be in src/ or specific modules
schema/                   → src/types/schema/
scripts/                  → src/scripts/ or archive/
templates/                → src/templates/ or archive/
packages/                 → archive/legacy-code/ (if unused)
verification_system/      → src/testing/verification/ or archive/
```

## 🚀 **Established Archival Script Procedure**

### **Step 1: Create Archive Structure**
```bash
mkdir -p archive/{legacy-docs,legacy-code,test-artifacts,build-artifacts}
mkdir -p archive/by-date/$(date +%Y-%m-%d)-root-cleanup
```

### **Step 2: Systematic Archival by Category**
```bash
# Documentation files
mv ACADEMY_*.md ARCHITECTURE.md BROWSER_*.md archive/legacy-docs/
mv CHECKIN_*.md CLEANUP_PLAN.md CONTINUUM_MANIFESTO.md archive/legacy-docs/
mv DEVTOOLS_*.md DOCUMENTATION_INDEX.md FILES.md archive/legacy-docs/
mv IMPLEMENTATION_ROADMAP.md INTEGRATION_STATUS.md archive/legacy-docs/
mv LANGUAGE_MIGRATION.md MESH_CHARTER.md MIDDLE_OUT_SUCCESS.md archive/legacy-docs/
mv MIGRATION_*.md MISSION.md PERSONA-*.md archive/legacy-docs/
mv README-*.md RESTORATION-STRATEGY.md ROADMAP.md archive/legacy-docs/
mv SCREENSHOT_*.md VALIDATION_COMPLETE.md VISION.md archive/legacy-docs/
mv WORKING_NOTES.md archive/legacy-docs/

# Legacy code directories
mv academy-sdk agent-scripts agents archived examples archive/legacy-code/
mv python-client temp-disabled verification archive/legacy-code/
mv packages templates verification_system archive/legacy-code/

# Test debris  
mv test-* debug-* junk-* archive/test-artifacts/
mv quick_commit_check_legacy.py archive/test-artifacts/

# Build artifacts (should also be .gitignored)
mv *.log build coverage dist archive/build-artifacts/
mv scripts schema archive/legacy-code/
```

### **Step 3: Update .gitignore**
```bash
# Add to .gitignore to prevent future pollution
echo "
# Build artifacts
*.log
build/
coverage/
dist/
node_modules/

# Test debris  
test-*
debug-*
temp-*

# Archive directory
archive/
" >> .gitignore
```

### **Step 4: Document Archive Contents**
```bash
# Create archive manifest
cat > archive/ARCHIVE_MANIFEST.md << 'EOF'
# Archive Manifest - Root Cleanup $(date +%Y-%m-%d)

## Archived Content Summary
- **Legacy Documentation**: 24 files moved from root to archive/legacy-docs/
- **Legacy Code**: 8 directories moved to archive/legacy-code/  
- **Test Artifacts**: 15+ files moved to archive/test-artifacts/
- **Build Artifacts**: Multiple log files and build directories

## Recovery Instructions
If any archived file is needed:
1. Check archive/by-date/YYYY-MM-DD-root-cleanup/ for dated backup
2. Original locations documented in this manifest
3. Consider if file should be restored to src/ vs middle-out/ vs permanently archived

## Archive Rationale
All files archived according to "nothing goes in root" principle.
Essential infrastructure files remain in root as documented in DECLUTTERING_PROTOCOL.md
EOF
```

## 📋 **Migration Mapping for Important Files**

### **Documentation Migration Targets**
| Current Location | New Location | Reason |
|------------------|--------------|---------|
| `ACADEMY_*.md` | `middle-out/academy/` | Academy system documentation |
| `AI-POWERED-DEVELOPMENT.md` | `middle-out/development/` | Development methodology |
| `ARCHITECTURE.md` | `middle-out/architecture/` | Core architecture docs |
| `BROWSER_LOGS_*.md` | `middle-out/jtag/` | Debugging documentation |
| `CONTINUUM_MANIFESTO.md` | `middle-out/research/` | Research and vision |
| `DEVTOOLS_*.md` | `middle-out/jtag/` | JTAG debugging system |
| `ROADMAP.md` | `middle-out/development/` | Development planning |

### **Code Migration Targets**
| Current Location | New Location | Reason |
|------------------|--------------|---------|
| `examples/` | `src/examples/` | Code belongs in src/ |
| `python-client/` | `src/integrations/python/` | Integration code |
| `scripts/` | `src/scripts/` | Build and utility scripts |
| `schema/` | `src/types/schema/` | Type definitions |
| `verification_system/` | `src/testing/verification/` | Testing infrastructure |

## 🛡️ **Prevention Protocols**

### **Git Hooks Integration**
```bash
# Pre-commit hook check for root pollution
.husky/pre-commit:
#!/bin/sh
# Check for new files in root that violate policy
ALLOWED_ROOT_FILES="src|middle-out|package|tsconfig|jest|eslint|\.husky|\.continuum|\.git|README\.md|continuum$"
NEW_ROOT_FILES=$(git diff --cached --name-only | grep -v -E "^($ALLOWED_ROOT_FILES)" | grep -v "/" || true)

if [ -n "$NEW_ROOT_FILES" ]; then
  echo "🚨 ROOT POLLUTION DETECTED:"
  echo "$NEW_ROOT_FILES"
  echo ""
  echo "❌ Nothing goes in repository root except essential infrastructure."
  echo "📋 See middle-out/development/DECLUTTERING_PROTOCOL.md for guidance."
  echo "🔧 Move files to appropriate locations in src/ or middle-out/"
  exit 1
fi
```

### **Development Guidelines**
1. **New Documentation**: Always goes in `middle-out/` subdirectories
2. **New Code**: Always goes in `src/` with proper module structure  
3. **Build Artifacts**: Must be .gitignored, never committed
4. **Test Files**: Go in module-specific `test/` directories
5. **Configuration**: Module-specific configs in module directories

### **Regular Audits**
```bash
# Weekly root directory audit
npm run audit:root     # Check for new root pollution
npm run archive:suggest # Suggest files for archival
npm run clean:root     # Remove build artifacts and temp files
```

## 📊 **Success Metrics**

### **Target State**
- **Root Directory**: ≤12 essential infrastructure files
- **Documentation**: 100% in middle-out/ structure
- **Code**: 100% in src/ structure  
- **Build Artifacts**: 0% committed (100% .gitignored)
- **Archive**: Well-organized with manifest and recovery instructions

### **Current vs Target**
- **Current**: 70+ files/directories in root
- **Target**: 12 essential infrastructure files
- **Reduction**: 83% reduction required
- **Timeline**: 1 week for major cleanup, ongoing prevention

## 🎯 **Implementation Timeline**

### **Phase 1: Emergency Cleanup (Days 1-2)**
- Archive all legacy documentation to appropriate middle-out/ locations
- Move critical code to src/ structure
- Archive test debris and build artifacts
- Update .gitignore to prevent regression

### **Phase 2: Structure Optimization (Days 3-4)**
- Consolidate related files in proper module structure
- Create archive manifest and recovery documentation
- Test that all essential functionality still works after moves

### **Phase 3: Prevention Systems (Days 5-7)**
- Implement git hook root pollution detection
- Add automated auditing scripts
- Document protocols for future development
- Train team on "nothing in root" principle

---

## 💡 **Key Principles**

1. **"Nothing goes in root"** - Absolute rule with narrow exceptions
2. **Archive, don't delete** - Preserve history with organized archival
3. **Document migration paths** - Clear mapping for important files
4. **Prevent regression** - Git hooks and automated checking
5. **Modular organization** - Everything has a proper place in the architecture

**Result: Clean, navigable repository that follows modular architecture principles and reduces cognitive overhead for developers and AI agents.**