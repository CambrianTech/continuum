"""
Continuum Integration Test Configuration
Shared fixtures and utilities for integration testing
"""

import pytest
import asyncio
import logging
from typing import Generator
from continuum_client import ContinuumServerManager

# Configure test logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

@pytest.fixture(scope="session")
def event_loop() -> Generator[asyncio.AbstractEventLoop, None, None]:
    """Create an instance of the default event loop for the test session."""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()

@pytest.fixture(scope="session")
def continuum_server():
    """Session-scoped Continuum server for integration tests"""
    with ContinuumServerManager() as server:
        yield server

@pytest.fixture(autouse=True)
def check_virtual_env():
    """Automatically check for virtual environment on each test"""
    import sys
    import warnings
    
    if not (hasattr(sys, 'real_prefix') or (hasattr(sys, 'base_prefix') and sys.base_prefix != sys.prefix)):
        warnings.warn(
            "\n🚨 WARNING: Not running in virtual environment!\n"
            "This may cause dependency conflicts and test failures.\n"
            "Please run: python -m venv continuum_test_env && source continuum_test_env/bin/activate",
            UserWarning,
            stacklevel=2
        )

@pytest.fixture
def test_agent_config():
    """Standard test agent configuration"""
    return {
        'agentId': 'test-agent',
        'agentName': 'Test Agent',
        'agentType': 'ai',
        'capabilities': ['chat', 'ui-interaction', 'testing', 'dom-manipulation']
    }

@pytest.fixture
def fred_agent_config():
    """Fred agent configuration for Fred-specific tests"""
    return {
        'agentId': 'fred',
        'agentName': 'Fred',
        'agentType': 'ai',
        'capabilities': ['chat', 'ui-interaction', 'testing']
    }

# Test markers for organization
def pytest_configure(config):
    """Configure custom test markers"""
    config.addinivalue_line(
        "markers", "fred_agent: marks tests as Fred agent specific"
    )
    config.addinivalue_line(
        "markers", "promise_flow: marks tests as promise-based flow tests"
    )
    config.addinivalue_line(
        "markers", "crash_recovery: marks tests as crash recovery tests"
    )
    config.addinivalue_line(
        "markers", "html_parsing: marks tests as HTML parsing tests"
    )
    config.addinivalue_line(
        "markers", "integration: marks tests requiring real server"
    )