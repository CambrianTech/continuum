# Continuum Python Client

Promise-based Python client for Continuum WebSocket server with clean async JavaScript execution.

## Features

- 🔗 **Promise-like WebSocket API** - Clean async/await interface
- ⚡ **JavaScript Execution** - Execute JS in browser with return values
- 🎯 **Error Handling** - Proper promise rejection for JS errors  
- 🔍 **DOM Querying** - Built-in DOM manipulation methods
- 🤖 **Agent Support** - Register and communicate as AI agents
- 📦 **Clean Architecture** - Well-organized, testable code structure

## Installation

```bash
pip install -e .
```

## Quick Start

```python
import asyncio
from continuum_client import ContinuumClient

async def main():
    async with ContinuumClient() as client:
        # Execute JavaScript with return values
        title = await client.js.get_value("document.title")
        print(f"Page title: {title}")
        
        # Query DOM elements
        agents = await client.js.query_dom(".agent-item")
        print(f"Found {len(agents)} agents")
        
        # Handle errors gracefully
        try:
            result = await client.js.get_value("nonexistent.property")
        except JSExecutionError as e:
            print(f"JS Error: {e}")

asyncio.run(main())
```

## Agent Example

```python
async def create_fred_agent():
    async with ContinuumClient() as client:
        # Register as an agent
        await client.register_agent({
            'agentId': 'fred',
            'agentName': 'Fred',
            'agentType': 'ai',
            'capabilities': ['chat', 'ui-interaction']
        })
        
        # Send a message
        await client.send_message("Hello! I'm Fred.", 'fred')
        
        # Check if I appear in the UI
        fred_visible = await client.js.get_value('''
            const items = document.querySelectorAll(".agent-item");
            return Array.from(items).some(item => 
                item.textContent.includes("Fred")
            );
        ''')
        
        if fred_visible:
            await client.send_message("✅ I can see myself in the UI!", 'fred')
```

## Promise-Based Execution

The client provides true promise-like behavior:

```python
# Success case - promise resolves
result = await client.js.execute("return 2 + 2")
# result = {'success': True, 'result': 4, 'output': [], 'error': None}

# Error case - promise rejects  
try:
    await client.js.execute("return undefined.property")
except JSExecutionError as e:
    print(f"Promise rejected: {e}")
```

## API Reference

### ContinuumClient

Main client class for WebSocket connections.

```python
client = ContinuumClient(url="ws://localhost:9000", timeout=10.0)
await client.connect()
await client.register_agent(agent_info)
await client.send_message(message, agent_id, target_agent)
await client.disconnect()
```

### JSExecutor

JavaScript execution with promise support.

```python
# Execute with full result
result = await client.js.execute(js_code, timeout=10.0, expect_return=False)

# Get return value directly
value = await client.js.get_value(js_expression)

# Execute for side effects, get console output
output = await client.js.run(js_code)

# Query DOM elements
elements = await client.js.query_dom(selector, property=None)

# Wait for element to appear
await client.js.wait_for_element(selector, timeout=5.0)

# Batch multiple operations
results = await client.js.batch([
    {'code': 'return 2 + 2', 'expect_return': True},
    {'code': 'console.log("test")', 'expect_return': False}
])
```

## Error Handling

```python
from continuum_client import JSExecutionError, JSTimeoutError, JSSyntaxError

try:
    result = await client.js.get_value("problematic.code")
except JSTimeoutError:
    print("JavaScript execution timed out")
except JSSyntaxError as e:
    print(f"JavaScript syntax error: {e}")
except JSExecutionError as e:
    print(f"JavaScript runtime error: {e}")
```

## Running Tests

```bash
# Install dev dependencies
pip install -e .[dev]

# Run unit tests
pytest tests/unit/ -v

# Run integration tests  
pytest tests/integration/ -v

# Run all tests with coverage
pytest tests/ -v --cov=continuum_client
```

## Architecture

```
Python Script
    ↓ (WebSocket)
ContinuumClient  
    ↓ (routes messages)
JSExecutor
    ↓ (promise-like execution)
Browser JavaScript
    ↓ (resolve/reject)
WebSocket Response
    ↓ (routed back)
Python Promise Resolution
```

The system acts like a **post office** where:
- Each request gets a tracking number (execution ID)
- Messages are routed to the correct destination  
- Responses are delivered back to the original sender
- Errors are handled as promise rejections

## Contributing

1. Follow the clean architecture patterns
2. Add tests for new features
3. Use proper error handling with custom exceptions
4. Document public APIs clearly