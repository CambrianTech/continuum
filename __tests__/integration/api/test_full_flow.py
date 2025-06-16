"""
Integration tests for full promise-based flow
Tests the complete Python → WebSocket → Browser → Promise → WebSocket → Python cycle
"""

import pytest
import asyncio
import json
from unittest.mock import AsyncMock, patch

from continuum_client import ContinuumClient, JSExecutionError

class TestFullPromiseFlow:
    """Integration tests for complete promise flow"""
    
    @pytest.mark.asyncio
    async def test_end_to_end_success_flow(self):
        """Test complete success flow: Python → WS → Browser → Resolve → WS → Python"""
        # Arrange
        mock_ws = AsyncMock()
        mock_ws.recv = AsyncMock(side_effect=[
            "status",  # Initial status
            "banner",  # Banner message
            json.dumps({  # JavaScript execution result
                'type': 'js_executed',
                'success': True,
                'result': 'Hello from browser!',
                'output': [{'level': 'log', 'message': 'Test log'}],
                'error': None
            })
        ])
        
        # Act
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                # Simulate receiving the js_executed message
                js_result_message = {
                    'type': 'js_executed',
                    'success': True,
                    'result': 'Hello from browser!',
                    'output': [{'level': 'log', 'message': 'Test log'}],
                    'error': None
                }
                
                # Create a completed future to simulate the response
                future = asyncio.Future()
                future.set_result(js_result_message)
                client.js.pending_executions['test'] = future
                
                # Execute JavaScript
                task = asyncio.create_task(client.js.get_value('return "Hello from browser!"'))
                
                # Simulate response handling
                client.js.handle_ws_message(js_result_message)
                
                result = await task
                
                # Assert
                assert result == 'Hello from browser!'
                mock_ws.send.assert_called()  # JavaScript was sent
    
    @pytest.mark.asyncio
    async def test_end_to_end_error_flow(self):
        """Test complete error flow: Python → WS → Browser → Reject → WS → Python"""
        # Arrange
        mock_ws = AsyncMock()
        mock_ws.recv = AsyncMock(side_effect=[
            "status",
            "banner",
            json.dumps({
                'type': 'js_executed',
                'success': False,
                'result': None,
                'output': [],
                'error': 'ReferenceError: undefined_var is not defined'
            })
        ])
        
        # Act & Assert
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                # Simulate error response
                error_message = {
                    'type': 'js_executed',
                    'success': False,
                    'result': None,
                    'output': [],
                    'error': 'ReferenceError: undefined_var is not defined'
                }
                
                future = asyncio.Future()
                future.set_result(error_message)
                client.js.pending_executions['test'] = future
                
                with pytest.raises(JSExecutionError) as exc_info:
                    task = asyncio.create_task(client.js.get_value('return undefined_var'))
                    client.js.handle_ws_message(error_message)
                    await task
                
                assert "ReferenceError" in str(exc_info.value)
    
    @pytest.mark.asyncio
    async def test_agent_registration_and_js_execution(self):
        """Test agent registration followed by JavaScript execution"""
        # Arrange
        mock_ws = AsyncMock()
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        
        agent_info = {
            'agentId': 'test-fred',
            'agentName': 'Fred',
            'agentType': 'ai',
            'capabilities': ['testing', 'ui-interaction']
        }
        
        # Act
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                # Register agent
                await client.register_agent(agent_info)
                
                # Send message
                await client.send_message("Hello from Fred!", "test-fred")
                
                # Execute JavaScript to check UI
                js_response = {
                    'type': 'js_executed',
                    'success': True,
                    'result': 5,  # Number of agents
                    'output': [{'level': 'log', 'message': 'Agent count checked'}],
                    'error': None
                }
                
                future = asyncio.Future()
                future.set_result(js_response)
                client.js.pending_executions['test'] = future
                
                task = asyncio.create_task(client.js.get_value(
                    'document.querySelector("agent-selector")?.shadowRoot.querySelectorAll(".agent-item").length || 0'
                ))
                
                client.js.handle_ws_message(js_response)
                agent_count = await task
                
                # Assert
                assert agent_count == 5
                assert mock_ws.send.call_count >= 3  # Registration + message + JS execution
    
    @pytest.mark.asyncio
    async def test_dom_querying_flow(self):
        """Test DOM querying with promise resolution"""
        # Arrange
        mock_ws = AsyncMock()
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        
        dom_elements = [
            {'tagName': 'DIV', 'textContent': 'Fred', 'className': 'agent-item'},
            {'tagName': 'DIV', 'textContent': 'Joel', 'className': 'agent-item'}
        ]
        
        # Act
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                js_response = {
                    'type': 'js_executed',
                    'success': True,
                    'result': dom_elements,
                    'output': [],
                    'error': None
                }
                
                future = asyncio.Future()
                future.set_result(js_response)
                client.js.pending_executions['test'] = future
                
                task = asyncio.create_task(client.js.query_dom('.agent-item'))
                client.js.handle_ws_message(js_response)
                elements = await task
                
                # Assert
                assert len(elements) == 2
                assert elements[0]['textContent'] == 'Fred'
                assert elements[1]['textContent'] == 'Joel'
    
    @pytest.mark.asyncio
    async def test_promise_timeout_handling(self):
        """Test timeout handling in promise flow"""
        # Arrange
        mock_ws = AsyncMock()
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        
        # Act & Assert
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                # Don't simulate any response (will cause timeout)
                with pytest.raises(asyncio.TimeoutError):
                    await client.js.execute('return "test"', timeout=0.1)
    
    @pytest.mark.asyncio 
    async def test_multiple_concurrent_executions(self):
        """Test handling multiple concurrent JavaScript executions"""
        # Arrange
        mock_ws = AsyncMock()
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        
        responses = [
            {'type': 'js_executed', 'success': True, 'result': 'Result 1'},
            {'type': 'js_executed', 'success': True, 'result': 'Result 2'},
            {'type': 'js_executed', 'success': True, 'result': 'Result 3'}
        ]
        
        # Act
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                # Create multiple concurrent executions
                tasks = []
                for i, response in enumerate(responses):
                    future = asyncio.Future()
                    future.set_result(response)
                    client.js.pending_executions[f'test_{i}'] = future
                    
                    task = asyncio.create_task(client.js.get_value(f'return "Result {i+1}"'))
                    tasks.append(task)
                    
                    # Simulate response
                    client.js.handle_ws_message(response)
                
                results = await asyncio.gather(*tasks)
                
                # Assert
                assert len(results) == 3
                assert results[0] == 'Result 1'
                assert results[1] == 'Result 2'
                assert results[2] == 'Result 3'