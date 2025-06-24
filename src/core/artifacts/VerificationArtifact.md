# VerificationArtifact Documentation
## Git Commit Verification with Inheritance-Driven Architecture

VerificationArtifact extends BaseArtifact to provide specialized git commit verification with comprehensive diagnostic data collection, console evidence capture, and JTAG feedback loop integration.

## 🎯 Purpose

VerificationArtifact creates organized, structured verification records for every git commit, ensuring:
- **Complete audit trail** of verification process
- **Console evidence collection** for JTAG debugging
- **Test result tracking** with detailed metrics
- **Visual proof** through UI screenshots
- **Legacy compatibility** with existing verification tools

## 🏗️ Architecture

### Inheritance Structure
```
BaseArtifact (Foundation)
├── artifact.json           # Universal metadata
├── summary.txt             # Human-readable summary
├── logs/                   # Universal logging interface
│   ├── client.log         # Portal/DevTools client output
│   ├── server.log         # Continuum server output  
│   ├── console.log        # Browser console forwarded logs
│   └── errors.log         # Error capture and stack traces
└── screenshots/            # Visual proof capability
    └── ui-capture.png     # Verification UI state screenshot

VerificationArtifact (Git Specialization)
└── verification/           # Git verification specific data
    ├── commit_info.json    # Git commit metadata (SHA, message, files)
    ├── test_results.json   # Test execution results and timings
    ├── console_evidence.txt # Browser console test evidence
    └── verification_report.txt # Human-readable verification summary
```

### Directory Pattern
```
.continuum/artifacts/verification/2025/06/20250624_HHMMSS_SHA/
├── [BaseArtifact structure]
└── verification/
    ├── commit_info.json
    ├── test_results.json  
    ├── console_evidence.txt
    └── verification_report.txt
```

## 🔧 API Reference

### Constructor
```javascript
const artifact = new VerificationArtifact(commitSHA, basePath = '.continuum/artifacts');
```
- `commitSHA`: Git commit SHA for this verification
- `basePath`: Base directory for artifact storage (optional)

### Core Methods

#### setCommitContext(commitSHA, commitMessage, changedFiles)
Sets git commit context information.
```javascript
artifact.setCommitContext(
    'abc123def456',
    'Fix critical security vulnerability', 
    ['src/auth.js', 'tests/security.test.js']
);
```

#### addConsoleEvidence(logEntry)
Adds browser console evidence for JTAG feedback loop validation.
```javascript
artifact.addConsoleEvidence({
    level: 'log',
    message: '🎯 UUID_abc123_CONSOLE_LOG_STARTING'
});

// Or simple string
artifact.addConsoleEvidence('Test console output detected');
```

#### setTestResults(results)
Records test execution results with computed metrics.
```javascript
artifact.setTestResults({
    tests: ['test1', 'test2', 'test3'],
    passed: 3,
    failed: 0,
    duration: 1500
});
```

#### setVerificationStatus(status, reason)
Sets final verification status with detailed reason.
```javascript
artifact.setVerificationStatus('passed', 'All verification checks successful');
// status: 'pending' | 'passed' | 'failed'
```

#### async saveVerificationData()
Saves all verification data to organized file structure.
```javascript
await artifact.saveVerificationData();
```

#### async createLegacySymlink()
Creates backward-compatible symlink for legacy tools.
```javascript
await artifact.createLegacySymlink();
// Creates: .continuum/verification/latest -> current artifact
```

### Inherited Methods (from BaseArtifact)
```javascript
// Universal logging interface
await artifact.logClient('Portal connected successfully');
await artifact.logServer('Continuum server processing request');  
await artifact.logConsole('Browser console: Test completed');
await artifact.logError('Connection timeout', 'client');

// Visual proof
await artifact.addScreenshot('ui-state.png', '/path/to/source.png');

// Structure creation
await artifact.createStructure(); // Creates complete directory hierarchy
```

## 📊 Data Formats

### commit_info.json
```json
{
  "sha": "abc123def456789",
  "message": "Fix critical security vulnerability",
  "changedFiles": ["src/auth.js", "tests/security.test.js"],
  "timestamp": "2025-06-24T22:48:00.000Z",
  "author": "developer@company.com"
}
```

### test_results.json
```json
{
  "totalTests": 15,
  "passed": 14,
  "failed": 1,
  "duration": 2300,
  "timestamp": "2025-06-24T22:48:05.000Z"
}
```

### console_evidence.txt
```
# Browser Console Evidence - Git Verification

[2025-06-24T22:48:01.000Z] LOG: 🎯 UUID_abc123_CONSOLE_LOG_STARTING
[2025-06-24T22:48:01.100Z] LOG: 🤖 UUID_abc123_AGENT_MONITORING_OUTPUT  
[2025-06-24T22:48:01.200Z] LOG: 🎨 UUID_abc123_BACKGROUND_CHANGED
[2025-06-24T22:48:01.300Z] ERROR: ⚠️ UUID_abc123_INTENTIONAL_ERROR_TEST
[2025-06-24T22:48:01.400Z] LOG: ✅ UUID_abc123_JS_EXECUTION_COMPLETE
```

### verification_report.txt
```
CONTINUUM GIT VERIFICATION REPORT
=====================================

Verification ID: 20250624_224800_abc123de
Commit SHA: abc123def456789
Status: ✅ PASSED
Timestamp: 2025-06-24T22:48:05.000Z

COMMIT DETAILS:
Message: Fix critical security vulnerability
Changed Files: 2
Files: src/auth.js, tests/security.test.js

TEST EXECUTION:
Total Tests: 15
Passed: 14
Failed: 1
Duration: 2300ms

CONSOLE EVIDENCE:
Browser Console Entries: 5
JTAG Feedback Loop: OPERATIONAL

VERIFICATION SUMMARY:
✅ All systems operational - commit approved
```

## 🔄 Git Hook Integration

VerificationArtifact integrates with git hooks through a Python ↔ Node.js bridge:

### Python Git Hook (`quick_commit_check.py`)
```python
# Extract git context
commit_sha, commit_message, changed_files = get_git_context()

# Create VerificationArtifact via Node.js
artifact_path = create_node_verification_artifact(commit_sha, commit_message, changed_files)

# Run verification process
verification_result = run_verification()

# Update artifact with results
update_verification_artifact(artifact_path, verification_result, screenshot_path)
```

### Node.js Bridge Script
```javascript
const VerificationArtifact = require('./src/core/artifacts/VerificationArtifact.cjs');

async function createVerificationArtifact() {
    const artifact = new VerificationArtifact(commitSHA);
    artifact.setCommitContext(commitSHA, commitMessage, changedFiles);
    await artifact.createStructure();
    artifact.setVerificationStatus('pending', 'Starting git hook verification');
    await artifact.saveVerificationData();
    await artifact.createLegacySymlink();
    console.log(artifact.artifactPath);
}
```

## 🧪 JTAG Integration

VerificationArtifact provides complete JTAG (debugging) feedback loop:

### Console Evidence Collection
- **DevTools Protocol** captures all browser console output
- **Unique UUIDs** identify verification-specific console messages
- **Real-time forwarding** from browser to artifact storage
- **Multiple log levels** (log, warn, error) with timestamps

### Visual Proof
- **Automatic screenshots** of verification UI state
- **UI state capture** during verification process
- **Visual confirmation** of agent feedback mechanisms

### Test Integration
- **Test result tracking** with pass/fail metrics
- **Duration measurement** for performance monitoring  
- **Error categorization** for debugging assistance

## 🔧 Development Workflow

### 1. Local Development
```bash
# Test VerificationArtifact directly
node -e "
const VA = require('./src/core/artifacts/VerificationArtifact.cjs');
const artifact = new VA('test123');
artifact.setCommitContext('test123', 'Test commit', ['test.js']);
artifact.createStructure().then(() => console.log('Created:', artifact.artifactPath));
"
```

### 2. Git Hook Testing
```bash
# Test git hook manually
python quick_commit_check.py

# Verify artifact creation
ls -la .continuum/artifacts/verification/2025/06/
```

### 3. Console Evidence Validation
```bash
# Check console evidence capture
cat .continuum/artifacts/verification/latest/verification/console_evidence.txt

# Verify JTAG feedback loop
grep "UUID_" .continuum/artifacts/verification/latest/verification/console_evidence.txt
```

## 🚀 Extension Examples

### Custom Verification Logic
```javascript
class SecurityVerificationArtifact extends VerificationArtifact {
    getRequiredDirectories() {
        return [
            ...super.getRequiredDirectories(),
            'security'  // Add security-specific directory
        ];
    }
    
    async runSecurityScan() {
        // Add security-specific verification
        await this.createStructure();
        // Security scanning logic...
        await this.saveSecurityResults();
    }
}
```

### Portal Integration
```javascript
// Create verification artifact from portal session
const artifact = new VerificationArtifact(sessionId);
artifact.addConsoleEvidence(portalConsoleOutput);
artifact.setTestResults(portalTestResults);
await artifact.saveVerificationData();
```

## 📚 Related Files

- `BaseArtifact.cjs` - Foundation class
- `VerificationArtifact.test.cjs` - Comprehensive unit tests  
- `quick_commit_check.py` - Git hook implementation
- `README.md` - Overall artifact system documentation

## 🔍 Troubleshooting

### Common Issues

**Artifact not created during git hook:**
- Check Node.js script execution in git hook output
- Verify VerificationArtifact.cjs path is correct
- Ensure .continuum directory permissions

**Console evidence missing:**  
- Verify DevTools Protocol connection
- Check browser console forwarding setup
- Confirm UUID generation in verification process

**Legacy symlink broken:**
- Check createLegacySymlink() execution
- Verify .continuum/verification directory exists
- Confirm relative path calculation

### Debug Commands
```bash
# Manual artifact creation test
node .continuum/test_verification_artifact.cjs

# Check artifact structure
find .continuum/artifacts -name "*COMMIT_SHA*" -type d

# Verify console evidence
grep -r "UUID_" .continuum/artifacts/verification/

# Check git hook integration
python quick_commit_check.py 2>&1 | grep -E "(MILESTONE|ARTIFACT)"
```