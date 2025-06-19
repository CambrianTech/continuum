# DevTools Integration System

## Overview

The DevTools integration provides AI agents and personas with full browser automation capabilities through a modular, pluggable architecture. This system allows any layer of the Continuum stack to interact with browsers programmatically.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    AI Agents/Personas                       │
├─────────────────────────────────────────────────────────────┤
│  Python Client    │  CLI Interface  │  Web Interface       │
├─────────────────────────────────────────────────────────────┤
│              DevTools Server (HTTP/WebSocket)               │
├─────────────────────────────────────────────────────────────┤
│                     DevTools Core                           │
├─────────────────────────────────────────────────────────────┤
│  Chrome Adapter   │  WebKit Adapter │  Firefox Adapter     │
├─────────────────────────────────────────────────────────────┤
│ Chrome DevTools   │ Safari Remote   │  Firefox Remote      │
│    Protocol       │   Debugging     │    Debugging         │
└─────────────────────────────────────────────────────────────┘
```

## Usage Examples

### Python Client (AI Agents)
```python
from continuum_client import ContinuumClient

client = ContinuumClient()

# Direct DevTools access
client.devtools.take_screenshot("test.png")
client.devtools.execute_script("console.log('Hello from AI')")
client.devtools.click_element("#submit-button")

# Through Continuum commands
await client.send_command('devtools', {
    'action': 'screenshot',
    'filename': 'agent_view.png'
})
```

### Continuum Commands
```bash
# Command line usage
continuum devtools connect
continuum devtools screenshot --filename test.png --fullPage true
continuum devtools execute --script "document.title"
```

### HTTP API (Any Language)
```bash
# REST API usage
curl -X POST http://localhost:9001/api/connect -d '{"adapter": "chrome"}'
curl -X POST http://localhost:9001/api/screenshot -d '{"format": "png"}'
curl -X POST http://localhost:9001/api/execute -d '{"script": "console.log('Hello')"}'
```

## Browser Support

### Chrome/Chromium
- **Protocol**: Chrome DevTools Protocol
- **Port**: 9222 (default)
- **Features**: Full console, network, performance, screenshots
- **Start Chrome**: `chrome --remote-debugging-port=9222`

### Safari/WebKit
- **Protocol**: WebKit Remote Debugging
- **Port**: 9999 (configurable)
- **Features**: Console, basic automation, screenshots
- **Enable**: Safari → Develop → Allow Remote Automation

### Firefox
- **Protocol**: Firefox Remote Debugging
- **Port**: 6000 (configurable)  
- **Features**: Console, network monitoring
- **Start Firefox**: `firefox --start-debugger-server=6000`

## AI Agent Capabilities

### Visual Automation
- Take screenshots for analysis
- Visual element detection
- Page layout understanding
- UI state monitoring

### Interactive Automation
- Click elements
- Fill forms
- Navigate pages
- Scroll and focus

### Data Extraction
- Scrape content
- Monitor network traffic
- Extract structured data
- Real-time page monitoring

### Development & Testing
- Console log monitoring
- Error detection
- Performance metrics
- Automated testing

## Advanced Use Cases

### Autonomous Web Agent
```python
class WebAgent:
    def __init__(self):
        self.devtools = DevToolsClient()
        
    async def complete_task(self, objective):
        # Take screenshot to see current state
        await self.devtools.take_screenshot("current_state.png")
        
        # Analyze page content
        page_info = self.devtools.get_page_info()
        elements = self.devtools.find_elements("button, input, a")
        
        # Use AI to determine next action
        next_action = await self.ai_decide_action(objective, page_info, elements)
        
        # Execute action
        if next_action['type'] == 'click':
            await self.devtools.click_element(next_action['selector'])
        elif next_action['type'] == 'type':
            await self.devtools.set_element_value(
                next_action['selector'], 
                next_action['text']
            )
        
        # Repeat until objective complete
```

### Real-time Monitoring Agent
```python
class MonitoringAgent:
    def __init__(self):
        self.devtools = DevToolsClient()
        
    async def monitor_application(self):
        # Connect to browser
        await self.devtools.connect("chrome")
        
        # Set up real-time monitoring
        while True:
            # Check console for errors
            errors = self.devtools.get_console_logs(level="error")
            if errors:
                await self.handle_errors(errors)
            
            # Monitor performance
            metrics = await self.devtools.command("performance")
            if metrics['success']:
                await self.analyze_performance(metrics)
            
            # Take periodic screenshots
            await self.devtools.take_screenshot(f"monitor_{time.time()}.png")
            
            await asyncio.sleep(5)
```

### Debugger Integration
```python
class DebuggerAgent:
    def __init__(self):
        self.devtools = DevToolsClient()
        
    async def debug_application(self, breakpoints):
        # Set breakpoints
        for bp in breakpoints:
            await self.devtools.command("debugger", {
                "action": "setBreakpoint",
                "url": bp['url'],
                "line": bp['line']
            })
        
        # Monitor execution
        while True:
            state = await self.devtools.command("debugger", {"action": "getState"})
            if state['paused']:
                # Analyze stack, variables, etc.
                await self.analyze_debug_state(state)
                
                # AI decides whether to continue, step, etc.
                action = await self.ai_debug_decision(state)
                await self.devtools.command("debugger", action)
```

## Integration Points

### Continuum Commands
The DevTools system is fully integrated with Continuum's command system:
- `DEVTOOLS` command for all operations
- Event bus integration for real-time data
- WebSocket forwarding for live updates

### Python Client
Direct integration with the ContinuumClient:
- `client.devtools` property for immediate access
- Async/await support for all operations
- Error handling and retry logic

### Persona System
AI personas can use DevTools for:
- Visual understanding of web interfaces
- Automated interaction with web applications
- Real-time monitoring and alerting
- Development and testing assistance

## Configuration

### Environment Variables
```bash
DEVTOOLS_ENABLED=true
DEVTOOLS_PORT=9001
CHROME_DEBUG_PORT=9222
SAFARI_DEBUG_PORT=9999
FIREFOX_DEBUG_PORT=6000
```

### Continuum Config
```json
{
  "devtools": {
    "enabled": true,
    "adapters": ["chrome", "webkit", "firefox"],
    "autoConnect": "chrome",
    "screenshotPath": "./screenshots",
    "logLevel": "info"
  }
}
```

## Future Enhancements

### Planned Features
- **Multi-browser orchestration**: Control multiple browsers simultaneously
- **Visual AI integration**: Computer vision for element detection
- **Natural language interface**: "Click the blue button" → automated execution
- **Collaborative debugging**: Multiple agents working together
- **Performance optimization**: Automated performance tuning
- **Accessibility testing**: Automated accessibility validation

### Extension Points
- Custom adapters for other browsers/tools
- AI model integration for visual understanding
- Plugin system for specialized automation tasks
- Integration with testing frameworks
- Connection to cloud browser services

## Security Considerations

- DevTools access requires explicit browser configuration
- Local-only by default (localhost binding)
- Command validation and sanitization
- Rate limiting for automation requests
- Audit logging for all DevTools operations

This system provides the foundation for truly autonomous web agents that can see, understand, and interact with web applications just like humans do, but with the precision and consistency of AI automation.