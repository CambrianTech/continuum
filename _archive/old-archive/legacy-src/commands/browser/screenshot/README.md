# Screenshot Command

**🎯 RemoteCommand**: Browser-server orchestrated screenshot capture with unified artifact system integration. Demonstrates proper command chaining and session-aware file handling.

## Definition
**Name**: screenshot
**Description**: Capture browser screenshot with advanced targeting
**Icon**: 📸
**Category**: Browser
**Status**: Active

## Parameters
- `selector`: CSS selector to target for screenshot
- `filename`: Output filename for the screenshot
- `format`: Image format (png, jpg, jpeg, webp)
- `destination`: Where to save (file, bytes, both)
- `animation`: Animation type (none, visible, animated)
- `subdirectory`: Subdirectory to save the screenshot in

## 🚀 Usage

### Command Interface
```bash
# Basic usage
continuum screenshot

# With options (customize based on your module)
continuum screenshot --help
continuum screenshot --verbose
```

### Programmatic Usage
```typescript
import { ScreenshotCommand } from './ScreenshotCommand.js';

// Execute the command
const result = await ScreenshotCommand.execute({
  // Add your parameters here
});

console.log(result);
```

## ⚙️ Configuration

```json
{
  "command": "screenshot",
  "category": "Browser",
  "capabilities": [
    "browser-control",
    "devtools-integration"
  ],
  "dependencies": [
    "base-command"
  ],
  "interfaces": [
    "command-bus",
    "browser-management"
  ],
  "permissions": [
    "browser-control",
    "devtools-access",
    "create-files",
    "write"
  ]
}
```

## 🧪 Testing

```bash
# Run all tests
npm test

# Run specific test types
npm run test:unit
npm run test:integration

# Validate module compliance
npm run validate
```

## 🏗️ Development

This module follows the Continuum modular architecture:

- **Self-validating**: Module validates its own compliance
- **Middle-out**: Tests from core outward 
- **Object-oriented**: Inherits from base classes
- **Migration-ready**: Can upgrade structure automatically

### Module Structure
```
screenshot/
├── ScreenshotCommand.ts     # Main implementation
├── test/
│   ├── unit/             # Unit tests
│   └── integration/      # Integration tests
├── package.json          # Module configuration
└── README.md            # This file
```

## 🏗️ **RemoteCommand Architecture & Artifact Integration**

ScreenshotCommand demonstrates the **complete RemoteCommand pattern** with proper **command chaining** and **unified artifact system** integration:

### **RemoteCommand Pattern Implementation**
```typescript
class ScreenshotCommand extends RemoteCommand {
  // 1. Server prepares request for browser execution
  protected static async prepareForRemoteExecution(params, context) {
    // Validates parameters, infers format from filename
    // Returns standardized RemoteExecutionRequest
  }
  
  // 2. Browser-side execution with html2canvas
  protected static async executeOnClient(request) {
    // Runs in browser context
    // Uses html2canvas for DOM screenshot capture
    // Returns image data + metadata
  }
  
  // 3. Server processes response with artifact integration
  protected static async processClientResponse(response, originalParams) {
    // Uses strongly typed enums for behavior control
    // Integrates with unified artifact system
    // Returns appropriate result based on destination mode
  }
}
```

### **Strongly Typed Behavior Control**
```typescript
export enum ScreenshotDestination {
  FILE = 'file',           // Save to session artifacts, return filename
  BYTES = 'bytes',         // Return raw image data only
  BOTH = 'both'           // Save to artifacts AND return bytes
}

export enum ScreenshotFormat {
  PNG = 'png',            // Inferred from filename extension
  JPG = 'jpg', 
  JPEG = 'jpeg',
  WEBP = 'webp'
}

export enum ScreenshotAnimation {
  NONE = 'none',          // No UI feedback
  VISIBLE = 'visible',    // Show ROI highlighting
  ANIMATED = 'animated'   // Animate ROI highlighting
}
```

### **Command Chaining vs Direct Operations**
**❌ Old Approach (Direct fs operations):**
```typescript
// Anti-pattern: Direct fs operations create tight coupling
const buffer = Buffer.from(base64Data, 'base64');
fs.writeFileSync(fullPath, buffer);  // ❌ Circular dependencies
```

**✅ New Approach (Kernel service integration):**
```typescript
// Proper pattern: Delegate to kernel services
return this.createSuccessResult('Screenshot captured', {
  artifact: {
    type: ArtifactType.SCREENSHOT,
    content: base64Data,
    metadata: {
      command: 'screenshot',
      filename: params.filename,
      category: 'ui-validation'
    }
  }
});
// SessionManager + ContinuumDirectoryDaemon handle file operations
```

### **Session-Aware Artifact Placement**
```bash
# AI Portal usage
python3 python-client/ai-portal.py --cmd screenshot --params '{"selector": ".main-ui"}'
# → .continuum/sessions/portal-2025-06-30-1843/artifacts/screenshots/main-ui_timestamp.png

# Git Hook usage  
continuum screenshot --selector="body" --filename="pre-commit-validation.png"
# → .continuum/sessions/git-hook-2025-06-30-1843/artifacts/screenshots/pre-commit-validation.png

# Interactive CLI usage
continuum screenshot --filename="test.png" --destination="both"
# → .continuum/sessions/interactive-2025-06-30-1843/artifacts/screenshots/test.png
# + Returns base64 image data for immediate use
```

### **AI-Optimized Usage Examples**
```typescript
// Low-res screenshot for AI analysis
{
  "selector": ".main-content",
  "destination": "bytes",
  "width": 400,
  "height": 300,
  "maxFileSize": 50000
}

// Widget screenshot with cropping
{
  "selector": ".widget-container",
  "filename": "widget.png",
  "cropX": 10,
  "cropY": 10,
  "cropWidth": 300,
  "cropHeight": 200,
  "elementName": "settings-widget"
}

// High-quality design screenshot
{
  "filename": "ui-design.png",
  "destination": "both",
  "scale": 2.0,
  "format": "png",
  "quality": 1.0
}

// AI script automation friendly
{
  "selector": "#button-after-click",
  "destination": "bytes",
  "width": 200,
  "height": 100,
  "elementName": "clicked-button-state"
}
```

### **Architecture Benefits**
- ✅ **No circular dependencies** - Uses kernel services, not other commands
- ✅ **Session awareness** - Automatically routes to appropriate session artifacts
- ✅ **Universal usage** - Same command works for Portal, Git Hook, CLI
- ✅ **Command chaining ready** - Returns both artifacts and data for composition
- ✅ **Strongly typed behavior** - Enums prevent runtime errors
- ✅ **RemoteCommand pattern** - Template for all browser-server coordination

## 🔧 Bootstrap Information

This file was auto-generated during module migration. The module now has:

- ✅ Complete package.json with continuum configuration
- ✅ Test directories (unit/integration)
- ✅ TypeScript ES module setup
- ✅ Compliance validation

**Next Steps**: Implement your module logic and update this documentation!