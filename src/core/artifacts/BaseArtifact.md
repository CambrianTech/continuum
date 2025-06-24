# BaseArtifact Documentation
## Foundation Class for Universal Diagnostic Architecture

BaseArtifact is the foundation class that defines the minimal required structure for ALL Continuum artifacts. It establishes universal patterns for logging, metadata, visual capture, and directory organization that every specialized artifact inherits.

## 🎯 Design Philosophy

**Inheritance drives both code behavior AND directory structure.**

BaseArtifact creates a universal foundation that:
- **Standardizes diagnostic data collection** across all contexts
- **Provides consistent interface** for AI agents and developers  
- **Enables modular extension** through inheritance patterns
- **Ensures future compatibility** through stable base structure

## 🏗️ Core Structure

### Required Directories
Every artifact automatically gets these directories:
```
artifact_path/
├── logs/              # Universal logging interface
│   ├── client.log     # Client-side activity (portal, DevTools)
│   ├── server.log     # Server-side activity (Continuum core)
│   ├── console.log    # Browser console forwarded logs
│   └── errors.log     # Error capture with stack traces
└── screenshots/       # Visual proof capability
    └── [captured UI states]
```

### Required Files
Every artifact automatically gets these files:
```
artifact_path/
├── artifact.json      # Structured metadata
└── summary.txt        # Human-readable summary
```

### Path Structure
```
.continuum/artifacts/{type}/{year}/{month}/{id}/
└── [BaseArtifact structure + specialized extensions]
```

## 🔧 API Reference

### Constructor
```javascript
const BaseArtifact = require('./BaseArtifact.cjs');
const artifact = new BaseArtifact(type, id, basePath);
```

**Parameters:**
- `type`: Artifact category ('verification', 'session', 'test', etc.)
- `id`: Unique identifier for this artifact instance
- `basePath`: Base directory for storage (default: '.continuum/artifacts')

**Generated Properties:**
- `artifact.type`: Artifact type
- `artifact.id`: Unique identifier  
- `artifact.timestamp`: Creation timestamp
- `artifact.artifactPath`: Full path to artifact directory
- `artifact.metadata`: Structured metadata object

### Core Methods

#### async createStructure()
Creates complete directory hierarchy and required files.
```javascript
await artifact.createStructure();
// Creates: logs/, screenshots/, artifact.json, summary.txt
// Calls: createExtendedStructure() for subclass customization
```

#### getRequiredDirectories()
Returns array of required directories. Override in subclasses.
```javascript
getRequiredDirectories() {
    return [
        'logs',        // Universal logging
        'screenshots'  // Visual capture
    ];
}

// In subclass:
getRequiredDirectories() {
    return [
        ...super.getRequiredDirectories(),
        'custom_dir'   // Add specialized directory
    ];
}
```

#### async createExtendedStructure()
Override in subclasses for type-specific structure creation.
```javascript
// BaseArtifact (default: no-op)
async createExtendedStructure() {
    // Subclasses override to add specialized structure
}

// In subclass:
async createExtendedStructure() {
    await this.ensureDir(path.join(this.artifactPath, 'specialized'));
    // Create specialized files...
}
```

### Universal Logging Interface

#### async logClient(message)
Log client-side activity (portal, DevTools, browser interactions).
```javascript
await artifact.logClient('Portal connected successfully');
await artifact.logClient('DevTools Protocol: Screenshot captured');
await artifact.logClient('Browser automation: Form submitted');
```

#### async logServer(message)  
Log server-side activity (Continuum core, command processing).
```javascript
await artifact.logServer('Command processed: screenshot');
await artifact.logServer('WebSocket connection established');
await artifact.logServer('Session created: portal_session_123');
```

#### async logConsole(message)
Log browser console output (forwarded from DevTools Protocol).
```javascript
await artifact.logConsole('🎯 UUID_abc123_CONSOLE_LOG_STARTING');
await artifact.logConsole('Test execution completed successfully');
await artifact.logConsole('Error: Connection timeout in module X');
```

#### async logError(error, source)
Log errors with automatic categorization and cross-referencing.
```javascript
await artifact.logError('Connection timeout', 'client');
await artifact.logError('Database query failed', 'server');
await artifact.logError('JavaScript exception: undefined variable', 'console');

// Automatically logs to:
// - errors.log (central error tracking)
// - {source}.log (contextual error placement)
```

#### async logCommand(command, result)
Log command execution with results.
```javascript
await artifact.logCommand('screenshot --filename test.png', 'SUCCESS');
await artifact.logCommand('browser_js "console.log(test)"', 'EXECUTED');
```

### Visual Capture Interface

#### async addScreenshot(filename, sourcePath)
Add screenshot with automatic organization.
```javascript
// Copy from existing file
await artifact.addScreenshot('ui-state.png', '/path/to/screenshot.png');

// Create placeholder for later population
await artifact.addScreenshot('verification-ui.png');
// Returns: full path to screenshot location
```

### Metadata Management

#### async updateStatus(status)
Update artifact status with automatic persistence.
```javascript
await artifact.updateStatus('PROCESSING');
await artifact.updateStatus('COMPLETED');
await artifact.updateStatus('FAILED');
```

#### async writeSummary(summaryText)
Write human-readable summary.
```javascript
await artifact.writeSummary(`
Git Verification - Commit abc123
Status: PASSED
Duration: 25.3s
Console Evidence: 9 entries
JTAG Feedback: OPERATIONAL
`);
```

#### async writeArtifactJson()
Write structured metadata to artifact.json (called automatically).
```javascript
// Automatic content:
{
  "id": "20250624_224800_abc123",
  "type": "verification", 
  "timestamp": "2025-06-24T22:48:00.000Z",
  "status": "COMPLETED",
  "summary": "Brief description",
  "version": "1.0.0",
  "continuum_version": "0.2.2127"
}
```

### Utility Methods

#### async ensureDir(dirPath)
Create directory with recursive option.
```javascript
await artifact.ensureDir(path.join(artifact.artifactPath, 'custom', 'nested'));
```

#### computePath(basePath)
Compute organized path with year/month structure.
```javascript
// Automatic path generation:
// .continuum/artifacts/verification/2025/06/20250624_224800_abc123/
```

## 🧬 Inheritance Patterns

### Basic Extension
```javascript
class CustomArtifact extends BaseArtifact {
    constructor(customId, basePath) {
        super('custom', customId, basePath);
        this.customProperty = 'value';
    }
    
    getRequiredDirectories() {
        return [
            ...super.getRequiredDirectories(), // logs/, screenshots/
            'custom_data',                     // Add custom directory
            'analysis'                         // Add analysis directory
        ];
    }
    
    async createExtendedStructure() {
        // Create custom files in specialized directories
        const customDir = path.join(this.artifactPath, 'custom_data');
        await fs.writeFile(path.join(customDir, 'config.json'), '{}');
    }
    
    async saveCustomData(data) {
        await this.createStructure();
        // Custom save logic...
        await this.writeSummary('Custom artifact completed');
    }
}
```

### Advanced Extension with Specialized Interface
```javascript
class PortalSessionArtifact extends BaseArtifact {
    constructor(sessionId) {
        super('session', sessionId);
        this.commands = [];
        this.websocketMessages = [];
    }
    
    getRequiredDirectories() {
        return [
            ...super.getRequiredDirectories(),
            'commands',    // Portal command history
            'websocket',   // WebSocket message logs
            'browser'      // Browser interaction logs
        ];
    }
    
    // Specialized logging methods
    async logPortalCommand(command, params, result) {
        this.commands.push({ command, params, result, timestamp: new Date() });
        await this.logClient(`COMMAND: ${command} -> ${result}`);
    }
    
    async logWebSocketMessage(direction, message) {
        this.websocketMessages.push({ direction, message, timestamp: new Date() });
        await this.appendLog('websocket.log', `${direction}: ${JSON.stringify(message)}`);
    }
    
    async saveSessionData() {
        await this.createStructure();
        
        // Save specialized data
        const commandsPath = path.join(this.artifactPath, 'commands', 'history.json');
        await fs.writeFile(commandsPath, JSON.stringify(this.commands, null, 2));
        
        const wsPath = path.join(this.artifactPath, 'websocket', 'messages.json');
        await fs.writeFile(wsPath, JSON.stringify(this.websocketMessages, null, 2));
        
        await this.writeSummary(this.generateSessionSummary());
    }
}
```

## 📊 Metadata Schema

### artifact.json Structure
```json
{
  "id": "20250624_224800_abc123",
  "type": "verification|session|test|custom",
  "timestamp": "2025-06-24T22:48:00.000Z",
  "status": "CREATED|PROCESSING|COMPLETED|FAILED",
  "summary": "Brief human-readable description",
  "version": "1.0.0",
  "continuum_version": "0.2.2127",
  "metadata": {
    // Subclass-specific metadata
    "custom_field": "value",
    "specialized_data": {...}
  }
}
```

### Log File Formats

#### client.log
```
[2025-06-24T22:48:01.000Z] Portal connected to localhost:9000
[2025-06-24T22:48:02.000Z] DevTools Protocol: Browser launched on port 9222
[2025-06-24T22:48:03.000Z] Screenshot captured: ui-state.png
[2025-06-24T22:48:04.000Z] Command completed: browser_js execution
```

#### server.log  
```
[2025-06-24T22:48:01.000Z] WebSocket connection established: client_123
[2025-06-24T22:48:02.000Z] Command received: screenshot --filename test.png
[2025-06-24T22:48:03.000Z] Command processed successfully
[2025-06-24T22:48:04.000Z] Response sent to client
```

#### console.log
```
[2025-06-24T22:48:01.000Z] 🎯 UUID_abc123_CONSOLE_LOG_STARTING
[2025-06-24T22:48:01.100Z] 🤖 UUID_abc123_AGENT_MONITORING_OUTPUT
[2025-06-24T22:48:01.200Z] Test execution completed successfully
[2025-06-24T22:48:01.300Z] ✅ UUID_abc123_JS_EXECUTION_COMPLETE
```

#### errors.log
```
[2025-06-24T22:48:05.000Z] ERROR: Connection timeout connecting to browser
[2025-06-24T22:48:06.000Z] ERROR: Screenshot capture failed: Permission denied
[2025-06-24T22:48:07.000Z] ERROR: Command execution timeout after 30s
```

## 🔧 Development Guidelines

### Creating New Artifact Types

1. **Extend BaseArtifact**
```javascript
class NewArtifact extends BaseArtifact {
    constructor(specificId) {
        super('new_type', specificId);
    }
}
```

2. **Define Required Directories**
```javascript
getRequiredDirectories() {
    return [
        ...super.getRequiredDirectories(),
        'specialized_dir1',
        'specialized_dir2'
    ];
}
```

3. **Implement Extended Structure**
```javascript
async createExtendedStructure() {
    // Create specialized files and subdirectories
}
```

4. **Add Specialized Methods**
```javascript
async saveSpecializedData(data) {
    await this.createStructure();
    // Implementation...
}
```

5. **Write Comprehensive Tests**
```javascript
// In __tests__/unit/js/artifacts/NewArtifact.test.cjs
describe('NewArtifact', () => {
    test('should extend BaseArtifact correctly', () => {
        const artifact = new NewArtifact('test123');
        expect(artifact).toBeInstanceOf(BaseArtifact);
    });
});
```

### Best Practices

1. **Always call super methods** when overriding
2. **Use inheritance for directory structure** - let the pattern drive organization
3. **Leverage universal logging** - use logClient, logServer, logConsole, logError
4. **Provide human-readable summaries** - always call writeSummary()
5. **Test inheritance patterns** - verify base functionality works in subclasses
6. **Document specialized interfaces** - create {ArtifactType}.md documentation

## 🚀 Integration Examples

### Git Hook Integration
```python
# Python wrapper for BaseArtifact subclasses
def create_artifact_via_node(artifact_type, artifact_id, data):
    script = f"""
    const {artifact_type} = require('./src/core/artifacts/{artifact_type}.cjs');
    const artifact = new {artifact_type}('{artifact_id}');
    // Set data and create structure...
    await artifact.createStructure();
    console.log(artifact.artifactPath);
    """
    result = subprocess.run(['node', '-e', script], capture_output=True, text=True)
    return result.stdout.strip()
```

### Portal Integration
```javascript
// In portal command processing
const PortalSessionArtifact = require('./src/core/artifacts/PortalSessionArtifact.cjs');

async function createPortalSession(sessionId) {
    const artifact = new PortalSessionArtifact(sessionId);
    await artifact.createStructure();
    await artifact.logClient('Portal session started');
    return artifact;
}
```

### Testing Integration
```javascript
// In test suites
const TestArtifact = require('./src/core/artifacts/TestArtifact.cjs');

async function runTestSuite(suiteId) {
    const artifact = new TestArtifact(suiteId);
    await artifact.createStructure();
    
    // Run tests...
    await artifact.logServer('Test suite completed');
    await artifact.saveTestResults(results);
}
```

## 📚 Related Documentation

- `README.md` - Overall artifact system architecture
- `VerificationArtifact.md` - Git verification specialization
- `__tests__/unit/js/artifacts/BaseArtifact.test.js` - Comprehensive test examples
- Individual artifact type documentation as the system grows

## 🔍 Troubleshooting

### Common Issues

**Directory creation fails:**
- Check permissions on .continuum directory
- Verify basePath is writable
- Ensure parent directories exist

**Logging not working:**
- Verify createStructure() was called first
- Check that logs/ directory exists
- Ensure proper async/await usage

**Inheritance problems:**
- Always call super() in constructor
- Use super.method() when overriding methods
- Verify require() paths are correct

### Debug Commands
```bash
# Check artifact structure
find .continuum/artifacts -type d | head -10

# Verify logging
cat .continuum/artifacts/*/latest/logs/client.log

# Check metadata
cat .continuum/artifacts/*/latest/artifact.json | jq .
```