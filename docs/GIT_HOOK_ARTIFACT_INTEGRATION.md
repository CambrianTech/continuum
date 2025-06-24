# Git Hook Artifact Integration
## Modern Verification with Inheritance-Driven Architecture

This document describes the integration between Continuum's git hooks and the inheritance-driven artifact system, showcasing how the VerificationArtifact provides comprehensive diagnostic data collection for every commit.

## 🎯 Overview

The git hook system has been modernized to use the VerificationArtifact inheritance architecture, replacing legacy flat-file verification with structured, organized diagnostic data collection.

### Before: Legacy Verification
```
verification/
├── verification_abc123/
│   ├── client-logs.txt
│   ├── server-logs.txt  
│   └── ui-capture.png
└── latest -> verification_abc123/
```

### After: Inheritance-Driven Architecture
```
.continuum/artifacts/verification/2025/06/20250624_HHMMSS_SHA/
├── artifact.json              # BaseArtifact metadata
├── summary.txt                 # BaseArtifact human summary
├── logs/                       # BaseArtifact universal logging
│   ├── client.log             # DevTools client activity
│   ├── server.log             # Continuum server activity
│   ├── console.log            # Browser console forwarding
│   └── errors.log             # Error tracking and debugging
├── screenshots/                # BaseArtifact visual proof
│   └── ui-capture.png         # Verification UI state
└── verification/               # VerificationArtifact specialization
    ├── commit_info.json        # Git commit metadata
    ├── test_results.json       # Test execution results
    ├── console_evidence.txt    # JTAG console evidence
    └── verification_report.txt # Human-readable verification summary
```

## 🏗️ Architecture Integration

### Python ↔ Node.js Bridge

The git hook (`quick_commit_check.py`) integrates with the Node.js VerificationArtifact through temporary bridge scripts:

```python
def create_node_verification_artifact(commit_sha, commit_message, changed_files):
    """Create VerificationArtifact using Node.js integration"""
    
    node_script = f"""
const VerificationArtifact = require('../src/core/artifacts/VerificationArtifact.cjs');

async function createVerificationArtifact() {{
    const artifact = new VerificationArtifact('{commit_sha}');
    
    // Set git context
    artifact.setCommitContext('{commit_sha}', '{commit_message}', {json.dumps(changed_files)});
    
    // Create directory structure
    await artifact.createStructure();
    
    // Set initial status
    artifact.setVerificationStatus('pending', 'Starting git hook verification');
    
    // Save basic structure
    await artifact.saveVerificationData();
    
    // Create legacy compatibility symlink
    await artifact.createLegacySymlink();
    
    // Output artifact path for Python to use
    console.log(artifact.artifactPath);
}}

createVerificationArtifact().catch(console.error);
"""
    
    # Execute Node.js script and return artifact path
    script_path = Path('.continuum/temp_verification_script.cjs')
    script_path.write_text(node_script)
    result = subprocess.run(['node', str(script_path)], capture_output=True, text=True)
    return result.stdout.strip()
```

## 🔄 Verification Workflow

### 1. Git Hook Trigger
```bash
# Git hook automatically executes during commit
git commit -m "Your commit message"
# ↓ Triggers: quick_commit_check.py
```

### 2. Context Extraction
```python
def get_git_context():
    """Extract git commit context information"""
    # Get current commit SHA
    sha_result = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
    commit_sha = sha_result.stdout.strip()
    
    # Get commit message  
    msg_result = subprocess.run(['git', 'log', '-1', '--pretty=format:%s'], capture_output=True, text=True)
    commit_message = msg_result.stdout.strip()
    
    # Get changed files
    files_result = subprocess.run(['git', 'diff', '--name-only', 'HEAD~1'], capture_output=True, text=True)
    changed_files = [f.strip() for f in files_result.stdout.split('\n') if f.strip()]
    
    return commit_sha, commit_message, changed_files
```

### 3. VerificationArtifact Creation
```javascript
// Node.js bridge script creates VerificationArtifact
const artifact = new VerificationArtifact(commitSHA);

// Set complete git context
artifact.setCommitContext(commitSHA, commitMessage, changedFiles);

// Create inheritance-driven structure
await artifact.createStructure();
// ↓ Creates: BaseArtifact foundation + VerificationArtifact specialization

// Set initial verification status
artifact.setVerificationStatus('pending', 'Starting git hook verification');

// Save structured data
await artifact.saveVerificationData();

// Create backward compatibility
await artifact.createLegacySymlink();
```

### 4. DevTools Verification Process
```python
def run_verification():
    """Run verification and return result"""
    # Launch DevTools browser for JTAG feedback
    result = subprocess.run([
        sys.executable, 'devtools_full_demo.py', '--commit-check'
    ], capture_output=True, text=True, timeout=60)
    
    # DevTools captures:
    # - Browser console output (JTAG evidence)
    # - UI screenshots (visual proof)
    # - JavaScript execution results
    # - WebSocket communication logs
    
    return result
```

### 5. Evidence Collection and Analysis
```python
def update_verification_artifact(artifact_path, verification_result, screenshot_path):
    """Update VerificationArtifact with verification results"""
    
    # Parse console evidence for JTAG validation
    console_evidence = []
    for line in verification_result.stdout.split('\n'):
        if 'UUID_' in line and any(keyword in line for keyword in 
                                 ['CONSOLE_LOG', 'AGENT_MONITORING', 'BACKGROUND_CHANGED']):
            console_evidence.append({"level": "log", "message": line.strip()})
    
    # Determine verification status
    if verification_result.returncode == 0:
        status = "passed"
        reason = "All verification checks passed"
        test_results = {"passed": 1, "failed": 0, "totalTests": 1}
    else:
        status = "failed" 
        reason = f"Verification failed with exit code {verification_result.returncode}"
        test_results = {"passed": 0, "failed": 1, "totalTests": 1}
```

### 6. Artifact Finalization
```javascript
// Node.js update script finalizes VerificationArtifact
const artifact = new VerificationArtifact(commitSha);
artifact.artifactPath = artifactPath; // Reconnect to existing artifact

// Add collected console evidence
for (const evidence of consoleEvidence) {
    artifact.addConsoleEvidence(evidence);
}

// Set final test results  
artifact.setTestResults(testResults);

// Set verification conclusion
artifact.setVerificationStatus(status, reason);

// Copy screenshot to artifact
if (screenshotPath) {
    const destPath = path.join(artifact.artifactPath, 'screenshots', 'ui-capture.png');
    await fs.promises.copyFile(screenshotPath, destPath);
}

// Save complete verification data
await artifact.saveVerificationData();
```

## 📊 Generated Verification Data

### Comprehensive Data Collection

Each git commit generates a complete diagnostic artifact:

#### Git Context (`verification/commit_info.json`)
```json
{
  "sha": "abc123def456789",
  "message": "Fix critical security vulnerability",
  "changedFiles": ["src/auth.js", "tests/security.test.js"],
  "timestamp": "2025-06-24T22:48:00.000Z",
  "author": "developer@company.com"
}
```

#### JTAG Evidence (`verification/console_evidence.txt`)
```
# Browser Console Evidence - Git Verification

[2025-06-24T22:48:01.000Z] LOG: 🎯 UUID_abc123_CONSOLE_LOG_STARTING
[2025-06-24T22:48:01.100Z] LOG: 🤖 UUID_abc123_AGENT_MONITORING_OUTPUT
[2025-06-24T22:48:01.200Z] LOG: 🎨 UUID_abc123_BACKGROUND_CHANGED
[2025-06-24T22:48:01.300Z] LOG: 📝 UUID_abc123_TITLE_CHANGED
[2025-06-24T22:48:01.400Z] LOG: 👁️ UUID_abc123_VISUAL_INDICATOR_ADDED
[2025-06-24T22:48:01.500Z] ERROR: ⚠️ UUID_abc123_INTENTIONAL_ERROR_TEST
[2025-06-24T22:48:01.600Z] WARNING: 🟡 UUID_abc123_INTENTIONAL_WARNING_TEST
[2025-06-24T22:48:01.700Z] LOG: ✅ UUID_abc123_JS_EXECUTION_COMPLETE
```

#### Verification Summary (`verification/verification_report.txt`)
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
Total Tests: 1
Passed: 1
Failed: 0
Duration: 24500ms

CONSOLE EVIDENCE:
Browser Console Entries: 8
JTAG Feedback Loop: OPERATIONAL

VERIFICATION SUMMARY:
✅ All systems operational - commit approved
```

#### Universal Logs (`logs/`)
```
client.log:     DevTools Protocol activity, browser interactions
server.log:     Continuum server processing, command execution  
console.log:    Browser console forwarding, real-time debugging
errors.log:     Error tracking, stack traces, debugging info
```

## 🔧 Configuration and Customization

### Git Hook Configuration
```python
# In quick_commit_check.py

# Artifact storage location
ARTIFACT_BASE_PATH = '.continuum/artifacts'

# DevTools verification script
VERIFICATION_SCRIPT = 'devtools_full_demo.py'

# Verification timeout
VERIFICATION_TIMEOUT = 60  # seconds

# Console evidence patterns
CONSOLE_EVIDENCE_PATTERNS = [
    'UUID_.*_CONSOLE_LOG',
    'UUID_.*_AGENT_MONITORING', 
    'UUID_.*_BACKGROUND_CHANGED',
    'UUID_.*_VISUAL_INDICATOR',
    'UUID_.*_JS_EXECUTION'
]
```

### VerificationArtifact Customization
```javascript
// Extend VerificationArtifact for specialized verification
class SecurityVerificationArtifact extends VerificationArtifact {
    getRequiredDirectories() {
        return [
            ...super.getRequiredDirectories(),
            'security'  // Add security scan results
        ];
    }
    
    async runSecurityAnalysis() {
        // Custom security verification logic
        await this.createStructure();
        // ... security scanning ...
        await this.saveSecurityResults();
    }
}
```

## 🧪 Testing and Validation

### Manual Testing
```bash
# Test git hook directly
python quick_commit_check.py

# Verify artifact creation
ls -la .continuum/artifacts/verification/2025/06/

# Check verification report
cat .continuum/artifacts/verification/latest/verification/verification_report.txt

# Validate console evidence
grep "UUID_" .continuum/artifacts/verification/latest/verification/console_evidence.txt
```

### Integration Testing
```bash
# Create test commit
echo "test" > test_file.txt
git add test_file.txt
git commit -m "Test verification artifact integration"

# Verify complete workflow
find .continuum/artifacts -name "*$(git rev-parse HEAD | cut -c1-8)*"

# Check all required files exist
ls -la .continuum/artifacts/verification/latest/
ls -la .continuum/artifacts/verification/latest/verification/
```

### Console Evidence Validation
```bash
# Verify JTAG feedback loop working
cat .continuum/artifacts/verification/latest/verification/console_evidence.txt | grep "UUID_"

# Expected output:
# [timestamp] LOG: 🎯 UUID_abc123_CONSOLE_LOG_STARTING
# [timestamp] LOG: 🤖 UUID_abc123_AGENT_MONITORING_OUTPUT
# [timestamp] LOG: ✅ UUID_abc123_JS_EXECUTION_COMPLETE
```

## 🚀 Benefits of Integration

### For Developers
- **Complete audit trail** for every commit
- **Rich debugging context** with console evidence
- **Visual proof** through automatic screenshots
- **Organized storage** with hierarchical structure
- **Legacy compatibility** during transition

### for AI Agents
- **Structured data access** through inheritance patterns
- **Universal logging interface** for consistent debugging
- **Rich metadata** for decision making
- **Modular extension** for specialized processing
- **Comprehensive context** for verification analysis

### For System Operations
- **Organized verification history** with year/month structure
- **Scalable storage** through hierarchical organization
- **Automated cleanup** through structured artifact lifecycle
- **Consistent interface** across all verification types
- **Future extensibility** through inheritance foundation

## 🔄 Migration from Legacy System

### Backward Compatibility
The new system maintains complete backward compatibility:

```bash
# Legacy tools continue to work
ls -la .continuum/verification/latest/
cat .continuum/verification/latest/ui-capture.png

# New structured access also available
ls -la .continuum/artifacts/verification/latest/
cat .continuum/artifacts/verification/latest/verification/verification_report.txt
```

### Migration Strategy
1. **Phase 1**: New system running alongside legacy (current)
2. **Phase 2**: Tools updated to use artifact system directly
3. **Phase 3**: Legacy compatibility layer removed (future)

### Tools Update Examples
```bash
# Legacy access
cat .continuum/verification/latest/client-logs.txt

# Modern structured access  
cat .continuum/artifacts/verification/latest/logs/client.log
cat .continuum/artifacts/verification/latest/verification/console_evidence.txt
```

## 📚 Related Documentation

- `src/core/artifacts/README.md` - Overall artifact system architecture
- `src/core/artifacts/BaseArtifact.md` - Foundation class documentation  
- `src/core/artifacts/VerificationArtifact.md` - Git verification specialization
- `devtools_full_demo.py` - DevTools Protocol verification implementation
- `quick_commit_check.py` - Complete git hook implementation

## 🔍 Troubleshooting

### Common Issues

**Git hook fails with Node.js error:**
- Check Node.js version compatibility
- Verify VerificationArtifact.cjs path is correct
- Ensure .continuum directory has write permissions

**Console evidence not captured:**
- Verify DevTools Protocol connection
- Check browser console forwarding setup
- Confirm UUID generation in verification script

**Artifact not created:**
- Check Python subprocess execution
- Verify Node.js bridge script syntax
- Ensure temporary script cleanup working

**Legacy symlink broken:**
- Check createLegacySymlink() execution
- Verify relative path calculation
- Confirm .continuum/verification directory exists

### Debug Commands
```bash
# Test Node.js bridge directly
node -e "
const VA = require('./src/core/artifacts/VerificationArtifact.cjs');
const artifact = new VA('debug123');
artifact.setCommitContext('debug123', 'Debug test', ['debug.txt']);
artifact.createStructure().then(() => console.log('Success:', artifact.artifactPath));
"

# Verify git hook workflow
python quick_commit_check.py 2>&1 | grep -E "(MILESTONE|ARTIFACT|ERROR)"

# Check artifact structure
find .continuum/artifacts -type f | head -20

# Validate console forwarding
tail -f .continuum/ai-portal/logs/buffer.log | grep "UUID_"
```