"""
Test UI updates and DOM verification after Fred registration
"""
import pytest
import asyncio
import logging
from unittest.mock import AsyncMock, patch

from continuum_client import ContinuumClient
from tests.fixtures import MockWebSocketServer

logger = logging.getLogger(__name__)

@pytest.mark.fred_agent
@pytest.mark.html_parsing
class TestUIUpdates:
    """Test UI updates after Fred registration"""
    
    @pytest.fixture
    async def fred_client(self):
        """WebSocket client for UI testing"""
        mock_ws = AsyncMock()
        mock_server = MockWebSocketServer()
        
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        mock_ws.send = AsyncMock()
        
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                client._mock_server = mock_server
                yield client, mock_server
    
    @pytest.mark.asyncio
    async def test_fred_appears_in_ui(self, fred_client):
        """Test Fred appears in agent selector UI"""
        client, mock_server = fred_client
        
        # Register Fred
        fred_config = {'agentId': 'fred', 'agentName': 'Fred', 'agentType': 'ai'}
        await client.register_agent(fred_config)
        
        logger.info("🔍 Testing UI updates...")
        
        # Query DOM for Fred
        js_response = mock_server.execute_js(
            'document.querySelectorAll(".agent-item")',
            'ui_check'
        )
        
        # Simulate promise resolution
        future = asyncio.Future()
        future.set_result(js_response)
        client.js.pending_executions['ui_check'] = future
        
        task = asyncio.create_task(client.js.query_dom('.agent-item'))
        client.js.handle_ws_message(js_response)
        ui_elements = await task
        
        # Verify Fred appears
        assert len(ui_elements) >= 1
        fred_element = next((el for el in ui_elements if el['textContent'] == 'Fred'), None)
        assert fred_element is not None
        assert fred_element['dataset']['agentId'] == 'fred'
        
        logger.info("✅ Fred appears correctly in UI")
    
    @pytest.mark.asyncio
    async def test_html_content_verification(self, fred_client):
        """Test HTML content after Fred registration"""
        client, mock_server = fred_client
        
        # Register Fred
        fred_config = {'agentId': 'fred', 'agentName': 'Fred', 'agentType': 'ai'}
        await client.register_agent(fred_config)
        
        # Get HTML content
        html_response = mock_server.execute_js(
            'document.querySelector("agent-selector").innerHTML',
            'html_check'
        )
        
        future = asyncio.Future()
        future.set_result(html_response)
        client.js.pending_executions['html_check'] = future
        
        task = asyncio.create_task(client.js.get_value('document.querySelector("agent-selector").innerHTML'))
        client.js.handle_ws_message(html_response)
        html_content = await task
        
        # Verify HTML structure
        assert 'agent-item' in html_content
        assert 'data-agent-id="fred"' in html_content
        assert 'Fred' in html_content
        
        logger.info("✅ HTML verification successful")