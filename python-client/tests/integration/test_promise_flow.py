"""
Test end-to-end promise-based execution flow
"""
import pytest
import asyncio
import logging
from unittest.mock import AsyncMock, patch

from continuum_client import ContinuumClient
from tests.fixtures import MockWebSocketServer

logger = logging.getLogger(__name__)

@pytest.mark.promise_flow
class TestPromiseFlow:
    """Test complete promise-based execution flows"""
    
    @pytest.fixture
    async def promise_client(self):
        """WebSocket client for promise testing"""
        mock_ws = AsyncMock()
        mock_server = MockWebSocketServer()
        
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        mock_ws.send = AsyncMock()
        
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                client._mock_server = mock_server
                yield client, mock_server
    
    @pytest.mark.asyncio
    async def test_concurrent_promise_operations(self, promise_client):
        """Test multiple concurrent promise operations"""
        client, mock_server = promise_client
        
        logger.info("🔄 Testing concurrent promises...")
        
        # Create multiple concurrent operations
        operations = [
            ('return "Operation 1"', 'op1'),
            ('return 2 + 2', 'op2'),
            ('return document.title || "Test"', 'op3'),
            ('return new Date().getTime()', 'op4')
        ]
        
        tasks = []
        for js_code, op_id in operations:
            # Mock response for each operation
            response = mock_server.execute_js(js_code, op_id)
            
            future = asyncio.Future()
            future.set_result(response)
            client.js.pending_executions[op_id] = future
            
            task = asyncio.create_task(client.js.get_value(js_code))
            tasks.append((task, response))
        
        # Simulate all responses
        for task, response in tasks:
            client.js.handle_ws_message(response)
        
        # Wait for all operations
        results = await asyncio.gather(*[task for task, _ in tasks], return_exceptions=True)
        
        # Verify all succeeded
        assert len(results) == 4
        assert all(not isinstance(result, Exception) for result in results)
        
        logger.info("✅ Concurrent promises completed")
    
    @pytest.mark.asyncio
    async def test_promise_timeout_handling(self, promise_client):
        """Test promise timeout scenarios"""
        client, mock_server = promise_client
        
        logger.info("⏱️ Testing promise timeouts...")
        
        # Don't simulate response (causes timeout)
        with pytest.raises(asyncio.TimeoutError):
            await client.js.execute('return "test"', timeout=0.1)
        
        logger.info("✅ Timeout handled correctly")