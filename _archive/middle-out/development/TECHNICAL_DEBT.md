# Technical Debt Registry - "If You See Something, Say Something"

## 🚨 **Critical Protocol: Zero Silent Debt**

> **"We need to be fully aware of found faults as we go about any task. If you see something say something"**

This document serves as the central registry for all identified technical debt, hardcoded values, brittle patterns, and architectural violations. Every issue is tracked, prioritized, and connected to actionable roadmaps.

## 📊 **Current Debt Inventory (Tracked Automatically)**

### 🔥 **P0 - Critical Issues (Fix Immediately)**

#### **🚨 ROOT DIRECTORY POLLUTION (HIGHEST PRIORITY)**
- **Issue**: 50+ loose files/directories in repository root violating "nothing goes in root" principle
- **Root Violations**:
  ```
  ACADEMY_*.md, ARCHITECTURE.md, BROWSER_*.md, CHECKIN_*.md, CLAUDE.md,
  CLEANUP_PLAN.md, CONTINUUM_MANIFESTO.md, DEVTOOLS_*.md, FILES.md,
  IMPLEMENTATION_ROADMAP.md, LANGUAGE_MIGRATION.md, MESH_CHARTER.md,
  MIDDLE_OUT_SUCCESS.md, MIGRATION_*.md, MISSION.md, PERSONA-*.md,
  README-*.md, RESTORATION-STRATEGY.md, ROADMAP.md, SCREENSHOT_*.md,
  TECHNICAL_DEBT.md, VALIDATION_COMPLETE.md, VISION.md, WORKING_NOTES.md
  ```
- **Legacy Archives**: `academy-sdk/`, `agent-scripts/`, `agents/`, `archived/`
- **Build Artifacts**: Multiple .log files, `build/`, `coverage/`, `dist/`
- **Test Debris**: `temp-disabled/`, `verification/`, loose test files
- **Impact**: Developer confusion, cognitive overhead, violates modular architecture
- **Fix**: Systematic archival using established script procedure
- **Roadmap**: Immediate decluttering sprint (within 1 week)

#### **Hardcoded Values & Magic Numbers**
- **File**: `src/commands/core/discovery/CommandDiscovery.ts:299-304`
  - **Issue**: Hardcoded file path patterns without fs.existsSync validation
  - **Code**: `possiblePaths = ['${basePath}/${capitalizedName}Command.ts', ...]`
  - **Impact**: Runtime errors if files don't exist, brittle command discovery
  - **Fix**: Dynamic file existence checking or npm intelligent resolution
  - **Roadmap**: Universal command loading patterns (Q4 2025)

#### **Brittle Type Casting**
- **File**: `src/commands/core/discovery/CommandDiscovery.ts:247-255`
  - **Issue**: `any` type usage in module export scanning
  - **Code**: `private findCommandClass(moduleExports: any): any`
  - **Impact**: No type safety, potential runtime failures
  - **Fix**: Proper interface definitions for command classes
  - **Roadmap**: Command interface standardization (Q4 2025)

#### **Missing Error Boundaries**
- **Pattern**: Command loading without proper error isolation
  - **Files**: Multiple command execution paths
  - **Issue**: Failed command loading can crash discovery system
  - **Impact**: System-wide failure from single command issue
  - **Fix**: Command isolation with error boundaries
  - **Roadmap**: Robust command loading patterns (Q4 2025)

### 🟡 **P1 - High Priority (Address Soon)**

#### **Cross-Cutting Dependencies**
- **File**: `src/integrations/academy/AcademyIntegration.ts`
  - **Issue**: Still contains hardcoded daemon references despite refactoring
  - **Code**: Direct daemon class imports vs dynamic discovery
  - **Impact**: Tight coupling, difficult testing, brittle integration
  - **Fix**: Complete dynamic discovery integration
  - **Roadmap**: Universal integration patterns (Q1 2026)

#### **God Object Patterns**
- **File**: `src/commands/core/discovery/CommandDiscovery.ts`
  - **Issue**: Single class handling discovery, loading, caching, and execution
  - **Code**: 300+ line class with multiple responsibilities
  - **Impact**: Hard to test, maintain, and extend
  - **Fix**: Split into CommandDiscovery, CommandLoader, CommandCache
  - **Roadmap**: Command system modularization (Q4 2025)

#### **Magic String Usage**
- **Pattern**: Category and type determination logic
  - **Files**: `src/commands/core/discovery/CommandDiscovery.ts:272-283`
  - **Issue**: Hardcoded category arrays and string matching
  - **Code**: `const coreCategories = ['core', 'system', 'kernel', 'base']`
  - **Impact**: Fragile categorization, requires code changes for new types
  - **Fix**: Configuration-driven categorization system
  - **Roadmap**: Dynamic command categorization (Q1 2026)

### 🟢 **P2 - Medium Priority (Improve When Possible)**

#### **Inconsistent Naming Conventions**
- **Pattern**: Mixed naming conventions across modules
  - **Issue**: PascalCase vs camelCase vs kebab-case inconsistency
  - **Files**: Various widget and command directories
  - **Impact**: Cognitive overhead, discovery confusion
  - **Fix**: Standardize on consistent naming patterns
  - **Roadmap**: Naming convention enforcement (Q2 2026)

#### **Duplicate Logic Patterns**
- **Pattern**: Error handling repetition across commands
  - **Issue**: Copy-paste error handling instead of shared utilities
  - **Files**: Multiple command implementations
  - **Impact**: Maintenance overhead, inconsistent behavior
  - **Fix**: Shared error handling utilities and patterns
  - **Roadmap**: Universal error handling (Q2 2026)

#### **Test Coverage Gaps**
- **Pattern**: Missing unit tests for discovery logic
  - **Files**: `src/commands/core/discovery/` needs comprehensive tests
  - **Issue**: Complex logic without proper test coverage
  - **Impact**: Refactoring risk, regression potential
  - **Fix**: Complete test suite for discovery system
  - **Roadmap**: Test coverage improvement (Q1 2026)

## 🔍 **Technical Debt Detection Patterns**

### **Automatic Detection Rules**

#### **Code Smell Patterns**
```bash
# Hardcoded values detection
grep -r "localhost:9000" src/           # Hardcoded ports
grep -r "'any'" src/                    # Unsafe type usage
grep -r "TODO" src/                     # Unfinished work
grep -r "FIXME" src/                    # Known issues
grep -r "hack" src/ -i                  # Temporary solutions
```

#### **Architectural Violations**
```bash
# Cross-cutting concerns
find src/ -name "*.ts" -exec grep -l "import.*\.\./\.\./\.\." {} \;

# God object indicators  
find src/ -name "*.ts" -exec wc -l {} \; | awk '$1 > 300'

# Circular dependency risks
npx madge --circular src/
```

#### **Quality Metrics**
```bash
# Type safety violations
grep -r ": any" src/
grep -r "as any" src/
grep -r "@ts-ignore" src/

# Error handling gaps
grep -r "catch" src/ | grep -v "error instanceof Error"
```

## 📋 **Immediate Action Items (Next Sprint)**

### **Quick Wins (< 2 hours each)**
1. **Add TODO markers** to all identified hardcoded values
2. **Extract magic strings** to configuration constants
3. **Add type interfaces** for command module exports
4. **Implement error boundaries** around command loading
5. **Create shared error utilities** for consistent handling

### **Medium Effort (2-8 hours each)**
1. **Split CommandDiscovery class** into focused components
2. **Implement dynamic categorization** system
3. **Add comprehensive tests** for discovery logic
4. **Create command loading abstractions** with proper error isolation
5. **Standardize naming conventions** across command modules

### **Major Refactoring (8+ hours each)**
1. **Complete academy integration** dynamic discovery
2. **Implement universal command interface** system
3. **Create configuration-driven** command system
4. **Build self-healing command loading** with fallbacks
5. **Design command composition** and chaining patterns

## 🎯 **Quality Assurance Integration**

### **Git Hook Integration**
```bash
# Pre-commit debt detection
.husky/pre-commit:
  - Run technical debt detection
  - Block commits that add new hardcoded values
  - Require TODO markers for temporary solutions
  - Validate no new 'any' types in clean modules
```

### **JTAG Debugging Integration**
```bash
# Real-time debt monitoring
./jtag debt-scan       # Scan for new technical debt
./jtag debt-report     # Generate debt report
./jtag debt-trends     # Track debt reduction over time
```

### **Documentation Requirements**
- **Every TODO** must reference this document section
- **Every hardcoded value** must have migration plan
- **Every 'any' type** must have interface replacement plan
- **Every architectural violation** must have refactoring timeline

## 🚀 **Debt Reduction Roadmap**

### **Q4 2025 - Foundation Cleanup**
- [ ] Eliminate all P0 critical issues
- [ ] Add comprehensive error boundaries
- [ ] Implement type-safe command interfaces
- [ ] Create shared utility libraries

### **Q1 2026 - Architecture Refinement**
- [ ] Complete modular command system
- [ ] Implement dynamic configuration systems
- [ ] Eliminate remaining hardcoded dependencies
- [ ] Achieve 100% type safety in core modules

### **Q2 2026 - Quality Excellence**
- [ ] Zero technical debt in core systems
- [ ] Self-healing architecture patterns
- [ ] Comprehensive test coverage (>95%)
- [ ] Automated debt prevention systems

## 🛡️ **Prevention Protocols**

### **"See Something, Say Something" Rules**
1. **When editing any file** - scan for debt patterns and document findings
2. **When adding new code** - check for existing patterns to reuse vs create
3. **When fixing bugs** - identify root cause debt and track for later fixing
4. **When reviewing code** - flag potential debt introduction
5. **When testing** - document brittle patterns discovered during testing

### **Debt Introduction Prevention**
```typescript
// GOOD: Configuration-driven
const config = getCommandConfig(commandName);
const filePath = config.filePath || generateFilePath(commandName);

// BAD: Hardcoded paths  
const filePath = `${basePath}/${commandName}Command.ts`; // TODO: Move to config

// GOOD: Type-safe interfaces
interface CommandModule {
  getDefinition(): CommandDefinition;
  execute(params: CommandParams): Promise<CommandResult>;
}

// BAD: Any types
private findCommandClass(moduleExports: any): any // TODO: Proper interfaces
```

### **Quality Ratchet Integration**
- **Graduated modules**: Zero tolerance for new debt
- **Candidate modules**: Debt reduction required for graduation
- **Whitelisted modules**: Debt tracking required, reduction encouraged

## 📊 **Metrics & Tracking**

### **Debt Metrics Dashboard**
```bash
# Weekly debt report
npm run debt:report     # Generate current debt inventory
npm run debt:trends     # Show debt reduction trends  
npm run debt:hotspots   # Identify highest-debt modules
npm run debt:impact     # Assess debt impact on development velocity
```

### **Success Criteria**
- **P0 issues**: 0 tolerance in production code
- **P1 issues**: <5 total across entire codebase
- **P2 issues**: Trend toward zero, no increase sprint-over-sprint
- **Detection time**: <24 hours from introduction to documentation
- **Resolution time**: P0 <1 week, P1 <1 month, P2 <1 quarter

---

## 🎯 **Remember: Technical Debt is an Investment Decision**

**Every piece of debt is:**
- **Consciously identified** and documented
- **Prioritized** by impact and effort
- **Tracked** through completion
- **Prevented** from accumulating silently

**The goal is not zero debt, but zero UNKNOWN debt and deliberate debt management aligned with business priorities.**