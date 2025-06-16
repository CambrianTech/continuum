# Test Strategy - Status Quo Approach

## Current Status ✅
- **JS Tests**: 400+ tests running via Jest 
- **Python Tests**: 47 found, but only 1 truly redundant
- **Core Coverage**: Screenshot, Commands, WebSocket, UI components covered in JS

## Strategy: Core-First, Minimal Python

### 1. Focus on JS/Core Tests (Primary)
- Keep all functionality in core JS tests
- These run reliably via `npm test`
- Good coverage of commands, components, integration

### 2. Python Tests Only When Necessary
- **Keep**: `python-client/test_screenshot.py` → Move to `__tests__/python/screenshot-client.py`
- **Keep**: Python-specific client tests (venv, imports, async)
- **Delete**: Tests that duplicate JS coverage

### 3. Simple npm test Integration
```json
"test": "jest && node __tests__/run-python-if-available.cjs"
```

### 4. Test Organization
```
__tests__/
├── unit/js/           # Main test coverage (Jest)
├── integration/       # JS integration tests  
├── functional/        # End-to-end JS tests
└── python/           # Python-specific only
    └── client-specific.py  # venv, async, client libs
```

## Immediate Actions
1. ✅ JS tests working (10/13 passing)
2. 🔄 Fix 3 failing JS test expectations
3. ➡️ Move 1 Python test to __tests__/python/
4. ➡️ Create python runner that fails gracefully

## Testing This Strategy
- `npm test` runs all JS tests reliably
- Python tests run if available, skip if not
- No redundant coverage
- Focus on core functionality first