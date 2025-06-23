# WebBrowse Command

## Definition
- **Name**: webbrowse
- **Description**: Browse websites, take screenshots, and interact with web content using DevTools Protocol
- **Category**: Browser
- **Icon**: 🌐
- **Status**: ✅ READY - DevTools-powered web browsing with screenshot support
- **Parameters**: `action`, `url`, `options`

## Overview
The WebBrowse command provides comprehensive website browsing capabilities using Chrome DevTools Protocol. It can navigate to any website, capture screenshots, interact with page elements, and extract content - all through an isolated browser instance.

## Core Features
- 🌐 **Universal Website Navigation** - Browse any website with full JavaScript support
- 📸 **High-Quality Screenshots** - DevTools Protocol screenshots of any webpage
- 🔧 **Page Interaction** - Click elements, fill forms, scroll, and interact with pages
- 📝 **Content Extraction** - Extract text, HTML, or specific page elements
- 🛡️ **Isolated Browsing** - Runs in dedicated browser instance with full isolation
- 🚀 **Credential Support** - Supports login workflows and authenticated browsing

## Parameters
- `action`: Action to perform (navigate, screenshot, click, extract, evaluate)
- `url`: Target URL (for navigate action)
- `options`: Additional options (selector, credentials, wait_time, etc.)

## Usage Examples

### Basic Navigation and Screenshots
```bash
# Navigate to a website
python3 ai-portal.py --cmd webbrowse --params '{"action": "navigate", "url": "https://example.com"}'

# Take screenshot of entire page
python3 ai-portal.py --cmd webbrowse --params '{"action": "screenshot", "options": {"filename": "example-site.png"}}'

# Navigate and screenshot in one command
python3 ai-portal.py --cmd webbrowse --params '{"action": "navigate", "url": "https://github.com", "options": {"screenshot": true, "filename": "github.png"}}'
```

### Content Extraction
```bash
# Extract page title
python3 ai-portal.py --cmd webbrowse --params '{"action": "extract", "options": {"selector": "title", "property": "textContent"}}'

# Extract all links on a page
python3 ai-portal.py --cmd webbrowse --params '{"action": "extract", "options": {"selector": "a", "property": "href", "all": true}}'

# Get page HTML
python3 ai-portal.py --cmd webbrowse --params '{"action": "extract", "options": {"selector": "body", "property": "innerHTML"}}'
```

### Page Interaction
```bash
# Click a button
python3 ai-portal.py --cmd webbrowse --params '{"action": "click", "options": {"selector": "#submit-button"}}'

# Fill a form field
python3 ai-portal.py --cmd webbrowse --params '{"action": "type", "options": {"selector": "#email", "text": "user@example.com"}}'

# Scroll to element
python3 ai-portal.py --cmd webbrowse --params '{"action": "scroll", "options": {"selector": "#target-section"}}'
```

### Advanced JavaScript Execution
```bash
# Execute custom JavaScript
python3 ai-portal.py --cmd webbrowse --params '{"action": "evaluate", "options": {"script": "document.querySelectorAll('img').length"}}'

# Wait for dynamic content
python3 ai-portal.py --cmd webbrowse --params '{"action": "wait", "options": {"selector": ".dynamic-content", "timeout": 5000}}'
```

## Package Rules
```json
{
  "timeouts": {"client": 30.0, "server": 25.0},
  "retries": {"client": 2, "server": 1},
  "behavior": {"client": "standard", "server": "devtools_browser"},
  "concurrency": {"client": true, "server": false},
  "sideEffects": ["launches_browser", "captures_screenshots", "navigates_web"]
}
```

## Technical Architecture

### DevTools Protocol Integration
- Uses Chrome DevTools Protocol for reliable browser automation
- Launches isolated Opera GX browser instance with debug port
- Real-time WebSocket communication for immediate response
- Comprehensive error handling and timeout management

### Screenshot System
- High-resolution screenshots using DevTools `Page.captureScreenshot`
- Supports full page, viewport, or element-specific captures
- Automatic file management with timestamp and custom naming
- Integration with Continuum's screenshot directory structure

### Security & Isolation
- Dedicated browser instance with isolated user data directory
- Configurable security policies and content restrictions
- Optional headless mode for server environments
- Clean process management with automatic cleanup

## Status Monitoring
The command provides detailed status information:
- Browser connection health
- Current page URL and title
- Screenshot capture success/failure
- Network request monitoring
- Console error detection

## Error Handling
- Automatic browser recovery on connection loss
- Retry mechanisms for network failures
- Comprehensive error reporting with context
- Graceful degradation when browser features unavailable

## Integration with Continuum
- Full integration with Continuum's screenshot directory
- WebSocket event broadcasting for real-time updates
- Logging integration for debugging and monitoring
- Command chaining support for complex workflows