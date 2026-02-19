# Exec Command - Enhanced with Timeout Protection

## Overview

The exec command allows execution of JavaScript code in the browser context with built-in timeout protection to prevent infinite loops and resource exhaustion.

## Key Features

### Timeout Protection
- **Default timeout**: 3 seconds (configurable)
- **Prevents infinite loops**: Automatically cancels execution that exceeds timeout
- **Memory protection**: Prevents browser from running out of memory
- **AbortController integration**: Provides cancellation signals to user code

### Usage

```bash
# Basic execution (uses 3-second default timeout)
./jtag exec --code='{"type":"inline","language":"javascript","source":"return 42;"}'

# Custom timeout (5 seconds)
./jtag exec --timeout=5000 --code='{"type":"inline","language":"javascript","source":"return 42;"}'
```

### User Code Cancellation Support

The exec command injects an `AbortSignal` into user code, allowing cooperative cancellation:

```javascript
// User code can check for cancellation
if (signal.aborted) {
    throw new Error('Execution cancelled');
}

// Long-running operations should periodically check
for (let i = 0; i < 1000000; i++) {
    if (signal.aborted) break;
    // ... do work
}
```

## Safety Features

1. **Loop Protection**: 3-second default timeout prevents infinite loops
2. **Memory Protection**: Execution is abandoned after timeout (though cleanup may continue)
3. **Error Classification**: Timeout errors are properly categorized as 'timeout' type
4. **Resource Cleanup**: Timers are properly cleared when execution completes normally

## Limitations

- **JavaScript execution cannot be forcefully killed**: Timeout protection abandons the promise but synchronous infinite loops may still consume CPU
- **Browser environment only**: Currently optimized for DOM manipulation and browser APIs
- **Single language support**: Only JavaScript execution is currently supported

## Implementation Details

- Uses `Promise.race()` between execution and timeout
- `AbortController` provides cancellation signaling
- Automatic timeout cleanup on successful completion
- Proper error type classification for debugging

## Security

- Code execution is sandboxed within the browser context
- No direct file system access (browser environment)
- Network requests subject to browser CORS policies
- DOM manipulation restricted to current page context