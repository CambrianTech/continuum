# Continuum Integration Test Suite

🚨 **VIRTUAL ENVIRONMENT MANDATORY** 🚨

## Quick Start

```bash
# 1. Create virtual environment (REQUIRED)
python -m venv continuum_test_env

# 2. Activate virtual environment  
source continuum_test_env/bin/activate  # Linux/Mac
# OR
continuum_test_env\Scripts\activate     # Windows

# 3. Install dependencies
pip install -e .
pip install -e .[dev]

# 4. Run all integration tests with managed server
python run_integration_tests.py --verbose --coverage
```

## Why Virtual Environment is Critical

- **Dependency Isolation**: Prevents WebSocket library conflicts
- **Async Compatibility**: Ensures correct asyncio library versions
- **Reproducible Tests**: Same environment every time
- **System Protection**: Won't contaminate global Python packages

## Test Coverage

### 🧩 Modular Test Suite

**Clean Architecture**: Each test file focuses on one concern.

| Test File | Focus | Promise Flow |
|-----------|-------|--------------|
| `test_fred_registration.py` | Agent registration & messaging | ✅ |
| `test_ui_updates.py` | UI updates & DOM queries | ✅ |
| `test_js_promise_errors.py` | Error handling & promise rejection | ✅ |
| `test_crash_recovery.py` | Server crashes & recovery | ✅ |
| `test_promise_flow.py` | Concurrent operations & timeouts | ✅ |
| `test_html_parsing.py` | HTML parsing & accessibility | ✅ |

### 🔄 Promise Architecture Verification

Each test validates the complete flow:
```
Python Script
    ↓ WebSocket Request
Continuum Server  
    ↓ Route to Browser
Browser JavaScript
    ↓ Promise Resolve/Reject
WebSocket Response
    ↓ Route Back (Post Office)
Python Promise Resolution
```

## Running Tests

### All Tests (Recommended)
```bash
python run_fred_tests.py --verbose --coverage --html-report
```

### Individual Test Categories
```bash
# Specific test modules
pytest tests/integration/test_fred_registration.py -v
pytest tests/integration/test_ui_updates.py -v

# Promise flow tests
pytest -m promise_flow -v

# Crash recovery tests  
pytest -m crash_recovery -v

# HTML parsing tests
pytest -m html_parsing -v
```

### With Coverage
```bash
pytest tests/ --cov=continuum_client --cov-report=html
# View: htmlcov/index.html
```

## Test Validation Points

### ✅ Agent Registration
- WebSocket message sent correctly
- Server acknowledges registration
- Agent appears in connected agents list

### ✅ UI Updates  
- Fred appears in agent selector
- CSS classes applied correctly
- Accessibility attributes present

### ✅ Promise-Based JS Execution
- JavaScript executes in browser
- Return values delivered to Python
- Errors properly rejected as exceptions

### ✅ HTML Parsing
- BeautifulSoup parses generated HTML
- DOM structure validated
- Element attributes verified

### ✅ Error Handling
- JavaScript syntax errors → JSExecutionError
- Network timeouts → JSTimeoutError  
- Promise rejections propagated correctly

### ✅ Crash Recovery
- Server restart simulation
- Connection recovery mechanisms
- Agent re-registration after crash

## Architecture Notes

### Mock WebSocket Server
- Simulates real Continuum server behavior
- Supports crash/restart scenarios
- Provides predictable test responses

### Execution ID Tracking (Post Office)
- Each JS execution gets unique ID
- Responses routed back to correct Python client
- Concurrent operations properly isolated

### HTML Verification
- Uses BeautifulSoup4 for robust parsing
- Validates accessibility attributes
- Checks CSS class applications

## Troubleshooting

### Virtual Environment Issues
```bash
# Check if in venv
python -c "import sys; print(hasattr(sys, 'real_prefix'))"

# Deactivate and recreate if needed
deactivate
rm -rf continuum_test_env
python -m venv continuum_test_env
source continuum_test_env/bin/activate
```

### Missing Dependencies
```bash
# Reinstall everything
pip install -e .[dev]
pip list | grep -E "pytest|websockets|beautifulsoup"
```

### Test Failures
```bash
# Run with maximum verbosity
python run_fred_tests.py --verbose

# Check individual test
pytest tests/integration/test_fred_registration.py::TestFredRegistration::test_fred_basic_registration -v -s
```

## Contributing

When adding new tests:

1. **Use the single connection pattern** from existing tests
2. **Add proper markers** (`@pytest.mark.fred_agent`, etc.)
3. **Include HTML verification** where applicable  
4. **Test both success and error cases**
5. **Verify promise-based flows** end-to-end

All tests should validate the complete Python → WebSocket → Browser → Promise → WebSocket → Python cycle.