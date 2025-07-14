# FileSave

FileSave command for saving binary data (like screenshots) to files with base64 support

## 🚀 Usage

### Command Interface
```bash
# Save base64 encoded screenshot
continuum file_save --content="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==" --filename="screenshot.png" --encoding="base64" --artifactType="screenshot"

# Save binary file
continuum file_save --content=<buffer> --filename="file.bin" --encoding="binary"

# Save to specific session
continuum file_save --content=<data> --filename="image.png" --sessionId="session123" --artifactType="screenshot"
```

### Programmatic Usage
```typescript
import { FileSaveCommand } from './FileSaveCommand.js';

// Save base64 encoded image
const result = await FileSaveCommand.execute({
  content: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==",
  filename: "screenshot.png",
  encoding: "base64",
  artifactType: "screenshot",
  sessionId: "session123"
});

console.log(result);
```

## ⚙️ Configuration

```json
// Add continuum configuration
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
fileSave/
├── FileSaveCommand.ts     # Main implementation
├── test/
│   ├── unit/             # Unit tests
│   └── integration/      # Integration tests
├── package.json          # Module configuration
└── README.md            # This file
```

## 📋 Implementation Notes

**TODO**: Customize this section with:
- Specific usage examples
- Configuration options
- API documentation
- Performance considerations
- Known limitations

## 🔧 Bootstrap Information

This file was auto-generated during module migration. The module now has:

- ✅ Complete package.json with continuum configuration
- ✅ Test directories (unit/integration)
- ✅ TypeScript ES module setup
- ✅ Compliance validation

**Next Steps**: Implement your module logic and update this documentation!