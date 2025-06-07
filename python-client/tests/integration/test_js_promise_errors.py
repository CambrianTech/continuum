"""
Test JavaScript error handling and promise rejection
"""
import pytest
import asyncio
import logging
from unittest.mock import AsyncMock, patch

from continuum_client import ContinuumClient, JSExecutionError
from tests.fixtures import MockWebSocketServer

logger = logging.getLogger(__name__)

@pytest.mark.promise_flow
class TestJSPromiseErrors:
    """Test JavaScript error handling with promise rejection"""
    
    @pytest.fixture
    async def client_with_server(self):
        """WebSocket client for error testing"""
        mock_ws = AsyncMock()
        mock_server = MockWebSocketServer()
        
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        mock_ws.send = AsyncMock()
        
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                client._mock_server = mock_server
                yield client, mock_server
    
    @pytest.mark.asyncio
    async def test_js_error_promise_rejection(self, client_with_server):
        """Test JavaScript error causes promise rejection"""
        client, mock_server = client_with_server
        
        logger.info("💥 Testing JS error handling...")
        
        # Execute JavaScript that will error
        error_response = mock_server.execute_js(
            'return undefined_variable.someProperty',
            'error_test'
        )
        
        future = asyncio.Future()
        future.set_result(error_response)
        client.js.pending_executions['error_test'] = future
        
        # Verify promise rejection
        with pytest.raises(JSExecutionError) as exc_info:
            task = asyncio.create_task(client.js.get_value('return undefined_variable.someProperty'))
            client.js.handle_ws_message(error_response)
            await task
        
        assert "ReferenceError" in str(exc_info.value)
        assert "undefined_variable" in str(exc_info.value)
        
        logger.info("✅ JS error properly rejected promise")
    
    @pytest.mark.asyncio
    async def test_js_syntax_error_handling(self, client_with_server):
        """Test JavaScript syntax error handling"""
        client, mock_server = client_with_server
        
        # Simulate syntax error
        syntax_error_response = {
            'type': 'js_executed',
            'success': False,
            'result': None,
            'error': 'SyntaxError: Unexpected token',
            'executionId': 'syntax_test'
        }
        
        future = asyncio.Future()
        future.set_result(syntax_error_response)
        client.js.pending_executions['syntax_test'] = future
        
        with pytest.raises(JSExecutionError) as exc_info:
            task = asyncio.create_task(client.js.get_value('return {invalid: syntax'))
            client.js.handle_ws_message(syntax_error_response)
            await task
        
        assert "SyntaxError" in str(exc_info.value)
        
        logger.info("✅ Syntax error handled correctly")