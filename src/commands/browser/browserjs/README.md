# Browser JS Command

## Definition
- **Name**: browser_js
- **Description**: Execute JavaScript code in browser context with security
- **Category**: Core
- **Icon**: 🔧
- **Status**: 🟠 UNTESTED (2025-06-18) - Needs documentation and testing
- **Parameters**: `[script] [encoding] [timeout] [returnResult]`

## Overview
The Browser JS command executes JavaScript code in the browser context with proper security measures. Supports base64 encoding to prevent injection attacks and can return execution results.

## Parameters
- `script`: JavaScript code to execute (or base64 encoded)
- `encoding`: Encoding type (base64 recommended for security)
- `timeout`: Execution timeout in seconds (default: 10)
- `returnResult`: Whether to return execution result (default: false)

## Usage Examples
```bash
# Execute simple JavaScript
python3 ai-portal.py --cmd browser_js --params '{"script": "console.log('Hello World');"}'

# Execute with base64 encoding (secure)
python3 ai-portal.py --cmd browser_js --params '{"script": "Y29uc29sZS5sb2coJ0hlbGxvIFdvcmxkJyk7", "encoding": "base64"}'

# Execute with result return
python3 ai-portal.py --cmd browser_js --params '{"script": "document.title", "returnResult": true}'

# Execute with custom timeout
python3 ai-portal.py --cmd browser_js --params '{"script": "console.log('test');", "timeout": 5}'
```

## Package Rules
```json
{
  "timeouts": {"client": 15.0, "server": 12.0},
  "retries": {"client": 1, "server": 0},
  "behavior": {"client": "standard", "server": "js_executor"},
  "concurrency": {"client": true, "server": true},
  "sideEffects": ["executes_browser_js", "modifies_dom"]
}
```

## Security Notes
- Base64 encoding is recommended to prevent injection attacks
- JavaScript execution is sandboxed within browser context
- Timeout prevents infinite loops and hanging scripts

## TODO:
- TODO: Test basic JavaScript execution
- TODO: Test base64 encoding security feature
- TODO: Test result return functionality
- TODO: Test timeout handling for long-running scripts
- TODO: Verify security sandbox prevents malicious code execution