# Incremental Migration Strategy

## Overview

Continuum's module structure migration is designed to be **zero-disruption** - both legacy and new patterns coexist without breaking existing functionality.

## Migration Principles

### 1. Path-Based Test Discovery
Tests are discovered by **path patterns**, not module structure:

```json
{
  "scripts": {
    "test:unit": "find src -name '*.test.ts' -path '*/test/unit/*' | xargs npx tsx --test",
    "test:integration": "find src -name '*.integration.test.ts' -path '*/test/integration/*' | xargs npx tsx --test"
  }
}
```

### 2. Coexistence Pattern
Both formats work simultaneously:

**Legacy format (still works):**
```
src/commands/browser/screenshot/test/unit/ScreenshotCommand.test.ts
src/commands/browser/screenshot/test/integration/ScreenshotWorkflow.integration.test.ts
```

**New format (also works):**
```
src/parsers/test/unit/ParserBase.test.ts
src/parsers/integrations/cli-parser/test/integration/CLIClientParser.integration.test.ts
```

### 3. Critical System Protection
- **Git hooks unchanged** - JTAG validation continues to work
- **Pre-commit tests** - All existing test paths remain valid
- **Build system** - No changes to compilation or bundling
- **CI/CD** - Existing pipelines continue to work

## Migration Phases

### Phase 1: New Modules (Current)
- New modules follow the pattern from start
- Example: `src/parsers/` module
- No impact on existing code

### Phase 2: Legacy Module Migration
- Existing modules gradually adopt the pattern
- Example: `src/commands/browser/screenshot/` reorganization
- Tests continue to work during migration

### Phase 3: Shared Code Cleanup
- Distribute scattered `src/shared/` and `src/types/` to proper modules
- Remove global dumping grounds
- Maintain backward compatibility

## Safety Mechanisms

### 1. Test Path Invariants
The test discovery patterns are **structure-agnostic**:
- `*/test/unit/*` - Works regardless of module organization
- `*/test/integration/*` - Works regardless of nesting level
- Pattern-based discovery vs. hardcoded paths

### 2. Import Path Stability
During migration, imports remain stable:
```typescript
// Before migration
import { ScreenshotTypes } from './ScreenshotTypes';

// After migration
import { ScreenshotTypes } from './shared/ScreenshotTypes';

// Transition period - both work with path mapping
```

### 3. Git Hook Compatibility
JTAG validation runs the same test commands:
```bash
# These commands work with both formats
npm test
npm run test:unit
npm run test:integration
```

## Implementation Strategy

### File Movement Protocol
1. **Copy** files to new locations (don't move yet)
2. **Update** new location imports
3. **Test** both old and new paths work
4. **Deprecate** old paths gradually
5. **Remove** old files only after validation

### Test Migration Example
```bash
# Old test still works
src/commands/browser/screenshot/test/unit/ScreenshotCommand.test.ts

# New test also works
src/commands/browser/screenshot/test/unit/ScreenshotCommand.test.ts

# Both discovered by: find src -name '*.test.ts' -path '*/test/unit/*'
```

### Import Path Evolution
```typescript
// Phase 1: Legacy paths (working)
import { ScreenshotTypes } from '../ScreenshotTypes';

// Phase 2: New paths (also working)
import { ScreenshotTypes } from '../shared/ScreenshotTypes';

// Phase 3: Consolidated (final)
import { ScreenshotTypes } from '../shared/ScreenshotTypes';
```

## Validation Checkpoints

### Pre-Migration Checklist
- [ ] All existing tests pass
- [ ] Git hooks function correctly
- [ ] Build system works
- [ ] JTAG validation succeeds

### Post-Migration Validation
- [ ] All tests still pass with new structure
- [ ] Git hooks still function
- [ ] Build system unaffected
- [ ] JTAG validation still succeeds
- [ ] New module tests integrated

### Rollback Strategy
If migration causes issues:
1. **Revert** file movements
2. **Restore** original import paths
3. **Validate** system returns to working state
4. **Analyze** failure points before retry

## Benefits of Incremental Approach

### 1. Zero Downtime
- Development continues uninterrupted
- No "big bang" migration risk
- Gradual improvement over time

### 2. Continuous Validation
- Each step validated before proceeding
- Git hooks ensure quality gates
- Test coverage maintained throughout

### 3. Reversible Changes
- Each migration step can be undone
- Clear rollback path if issues arise
- Minimal blast radius for problems

### 4. Team Productivity
- Developers can work in either format
- No coordination required across teams
- New patterns adopted naturally

## Success Metrics

### Technical Metrics
- **Test pass rate**: 100% maintained during migration
- **Build success**: No compilation failures
- **Git hook success**: JTAG validation continues to pass
- **CI/CD stability**: Pipeline success rates unchanged

### Architectural Metrics
- **Module consistency**: Increasing percentage following new pattern
- **Code organization**: Reduced cross-module dependencies
- **Test discoverability**: Improved test organization
- **Developer experience**: Faster onboarding to new modules

## Conclusion

The incremental migration strategy ensures that architectural improvements happen safely and continuously, without disrupting the critical development and validation workflows that keep Continuum stable and reliable.

This approach allows us to evolve the architecture while maintaining the high quality standards enforced by our git hooks and JTAG validation system.