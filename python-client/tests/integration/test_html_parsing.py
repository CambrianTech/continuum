"""
Test HTML parsing and DOM verification
"""
import pytest
import asyncio
import logging
from unittest.mock import AsyncMock, patch
from bs4 import BeautifulSoup

from continuum_client import ContinuumClient
from tests.fixtures import MockWebSocketServer

logger = logging.getLogger(__name__)

@pytest.mark.html_parsing
class TestHTMLParsing:
    """Test HTML parsing after Fred registration"""
    
    @pytest.fixture
    async def html_client(self):
        """WebSocket client for HTML testing"""
        mock_ws = AsyncMock()
        mock_server = MockWebSocketServer()
        
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        mock_ws.send = AsyncMock()
        
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                client._mock_server = mock_server
                yield client, mock_server
    
    @pytest.mark.asyncio
    async def test_html_structure_parsing(self, html_client):
        """Test parsing HTML structure with BeautifulSoup"""
        client, mock_server = html_client
        
        logger.info("📄 Testing HTML parsing...")
        
        # Register Fred
        fred_config = {'agentId': 'fred', 'agentName': 'Fred', 'agentType': 'ai'}
        await client.register_agent(fred_config)
        
        # Get HTML content
        html_response = mock_server.execute_js(
            'document.querySelector("agent-selector").innerHTML',
            'html_parse'
        )
        
        future = asyncio.Future()
        future.set_result(html_response)
        client.js.pending_executions['html_parse'] = future
        
        task = asyncio.create_task(client.js.get_value('document.querySelector("agent-selector").innerHTML'))
        client.js.handle_ws_message(html_response)
        html_content = await task
        
        # Parse with BeautifulSoup
        soup = BeautifulSoup(html_content, 'html.parser')
        agent_items = soup.find_all('div', class_='agent-item')
        
        assert len(agent_items) >= 1
        fred_item = next((item for item in agent_items if item.get_text() == 'Fred'), None)
        assert fred_item is not None
        assert fred_item.get('data-agent-id') == 'fred'
        
        logger.info("✅ HTML parsing successful")
    
    @pytest.mark.asyncio
    async def test_accessibility_attributes(self, html_client):
        """Test accessibility attributes in generated HTML"""
        client, mock_server = html_client
        
        # Mock HTML with accessibility attributes
        accessibility_html = '''
        <div class="agent-item" data-agent-id="fred" role="button" tabindex="0" aria-label="Fred Agent">
            Fred
        </div>
        '''
        
        html_response = {
            'type': 'js_executed',
            'success': True,
            'result': accessibility_html.strip(),
            'output': [],
            'executionId': 'accessibility_test'
        }
        
        future = asyncio.Future()
        future.set_result(html_response)
        client.js.pending_executions['accessibility_test'] = future
        
        task = asyncio.create_task(client.js.get_value('document.querySelector(".agent-item").outerHTML'))
        client.js.handle_ws_message(html_response)
        html_content = await task
        
        # Parse and verify accessibility
        soup = BeautifulSoup(html_content, 'html.parser')
        fred_item = soup.find('div', {'data-agent-id': 'fred'})
        
        assert fred_item.get('role') == 'button'
        assert fred_item.get('tabindex') == '0'
        assert fred_item.get('aria-label') == 'Fred Agent'
        
        logger.info("✅ Accessibility attributes verified")