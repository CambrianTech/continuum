# Continuum Artifact System
## Inheritance-Driven Universal Diagnostic Architecture

The Continuum Artifact System provides a sophisticated inheritance-driven architecture where **inheritance drives both code behavior AND directory structure**. This creates a universal diagnostic foundation that works across all contexts - git hooks, portal sessions, verification runs, and future extensions.

## 🏗️ Architecture Overview

```
BaseArtifact (Foundation)
├── Universal structure: artifact.json, summary.txt
├── Standard logging: logs/ directory with universal interface
├── Visual capture: screenshots/ directory for UI state proof
├── Metadata management: timestamp, status, type tracking
└── Modular methods: createStructure(), logging interface, utilities

VerificationArtifact extends BaseArtifact
├── Git specialization: commit context, console evidence, test results
├── Verification structure: verification/ subdirectory with specialized files
├── JTAG integration: browser console evidence collection
├── Status management: pending/passed/failed with detailed reasons
└── Legacy compatibility: creates verification/latest symlinks

[Future Artifacts]
PortalSessionArtifact extends BaseArtifact
DevToolsArtifact extends BaseArtifact
TestArtifact extends BaseArtifact
```

## 📁 Directory Structure Pattern

The inheritance pattern creates organized, hierarchical storage:

```
.continuum/artifacts/
├── verification/           # VerificationArtifact type
│   └── YYYY/MM/           # Year/Month organization
│       └── YYYYMMDD_HHMMSS_SHA/  # Artifact instance
│           ├── artifact.json      # BaseArtifact metadata
│           ├── summary.txt        # BaseArtifact summary
│           ├── logs/              # BaseArtifact logging
│           │   ├── client.log
│           │   ├── server.log
│           │   ├── console.log
│           │   └── errors.log
│           ├── screenshots/       # BaseArtifact visual proof
│           │   └── ui-capture.png
│           └── verification/      # VerificationArtifact specific
│               ├── commit_info.json
│               ├── test_results.json
│               ├── console_evidence.txt
│               └── verification_report.txt
├── sessions/              # Future: PortalSessionArtifact
├── tests/                 # Future: TestArtifact  
└── devtools/              # Future: DevToolsArtifact
```

## 🧬 Core Design Principles

### 1. Inheritance Drives Structure
- **Code inheritance** determines **directory inheritance**
- BaseArtifact creates foundation directories (logs/, screenshots/)
- Specialized artifacts add their own subdirectories
- Consistent pattern across all artifact types

### 2. Universal Diagnostic Interface
Every artifact inherits the same diagnostic capabilities:
```javascript
artifact.logClient(message)     // Client-side activity
artifact.logServer(message)     // Server-side activity  
artifact.logConsole(message)    // Browser console output
artifact.logError(error, source) // Error tracking
artifact.addScreenshot(filename, source) // Visual proof
```

### 3. Modular Extension Pattern
New artifact types extend BaseArtifact for specific use cases:
- Override `getRequiredDirectories()` to add specialized directories
- Override `createExtendedStructure()` for custom file creation
- Inherit all base functionality automatically

### 4. Legacy Compatibility
Artifacts maintain backward compatibility through:
- Symlinks to latest artifacts (`verification/latest`)
- Familiar file locations and naming
- Gradual migration path from legacy systems

## 🎯 Usage Examples

### Creating a VerificationArtifact
```javascript
const VerificationArtifact = require('./VerificationArtifact.cjs');

// Create artifact with commit SHA
const artifact = new VerificationArtifact('abc123def456');

// Set git context
artifact.setCommitContext('abc123def456', 'Fix critical bug', ['src/app.js']);

// Create complete structure
await artifact.createStructure();

// Add console evidence (JTAG feedback)
artifact.addConsoleEvidence({ level: 'log', message: 'Test passed' });

// Set test results
artifact.setTestResults({ passed: 5, failed: 0, duration: 1200 });

// Set final status
artifact.setVerificationStatus('passed', 'All tests successful');

// Save all data
await artifact.saveVerificationData();

// Create legacy compatibility
await artifact.createLegacySymlink();
```

### Extending for New Artifact Types
```javascript
class PortalSessionArtifact extends BaseArtifact {
    getRequiredDirectories() {
        return [
            ...super.getRequiredDirectories(), // logs/, screenshots/
            'commands',     // Portal command history
            'websocket',    // WebSocket message logs
            'browser'       // Browser state and interactions
        ];
    }
    
    async saveSessionData() {
        await this.createStructure();
        // Add session-specific file creation
    }
}
```

## 🔧 Integration with Git Hooks

The VerificationArtifact integrates seamlessly with git hooks through a Python ↔ Node.js bridge:

1. **Python git hook** (`quick_commit_check.py`) extracts git context
2. **Node.js script** creates VerificationArtifact with proper inheritance
3. **DevTools Protocol** captures browser console evidence  
4. **Python processing** updates artifact with verification results
5. **Legacy symlinks** maintain compatibility with existing tools

## 📊 Benefits

### For Developers
- **Consistent structure** across all diagnostic contexts
- **Rich metadata** for debugging and analysis
- **Visual proof** through automatic screenshots
- **Complete audit trail** of all activities

### For AI Agents  
- **Universal interface** for diagnostic data access
- **Inheritance patterns** easy to understand and extend
- **Modular design** enables specialized processing
- **Rich context** for decision making

### For System Operations
- **Organized storage** with hierarchical structure
- **Legacy compatibility** during transitions
- **Automated cleanup** through structured organization
- **Scalable architecture** for growing diagnostic needs

## 🚀 Future Extensions

The artifact system is designed for easy extension:

- **PortalSessionArtifact**: Complete portal session capture
- **TestArtifact**: Unit and integration test results
- **DevToolsArtifact**: Browser automation session data
- **DeploymentArtifact**: CI/CD pipeline diagnostic data
- **SecurityArtifact**: Security scan and audit results

Each extension follows the same inheritance pattern, ensuring consistency across the entire diagnostic ecosystem.

## 📚 Related Documentation

- `BaseArtifact.cjs` - Foundation class implementation
- `VerificationArtifact.cjs` - Git verification specialization
- `__tests__/unit/js/artifacts/` - Comprehensive unit tests
- `quick_commit_check.py` - Git hook integration example