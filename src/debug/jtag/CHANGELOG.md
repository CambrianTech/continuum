# Changelog

## [1.0.0] - 2025-07-20

### Added
- **Universal Debugging System**: One import enables console routing + WebSocket logging + screenshots
- **Zero Configuration**: Auto-initialization with graceful degradation 
- **Cross-Context Support**: Works in both Node.js and browser environments
- **Integration Test Chain**: Example apps become live test harnesses
- **Production-Grade Robustness**: Never breaks importing applications
- **TypeScript Support**: Full type safety throughout
- **WebSocket Transport**: Real-time browser → server log routing
- **Screenshot Capture**: Both server and browser screenshot capabilities
- **Console Interception**: Automatic console.log/error/warn routing
- **File Logging**: Structured logs in JSON and text formats
- **NPM Lifecycle Integration**: Tests run complete system validation

### Core Features
- `jtag.log(component, message, data)` - Structured logging
- `jtag.critical(component, message, data)` - Critical event logging  
- `jtag.screenshot(filename, options)` - Screenshot capture
- `jtag.exec(code)` - Code execution with timing
- `jtag.getUUID()` - UUID generation and tracking
- Automatic console routing: `console.log()` → log files

### Architecture
- Hybrid standalone/integrated module design
- Layer-based testing (1-6 layers from foundation to browser integration)
- Self-validating through proper API usage in examples
- Graceful degradation when components fail
- Middle-out architectural principles

### Testing
- Complete integration test chain via `npm test`
- Real browser automation with Puppeteer
- Cross-context validation (browser ↔ server)
- Production-mode testing with live WebSocket communication