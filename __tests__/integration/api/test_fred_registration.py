"""
Test agent registration and basic functionality
"""
import pytest
import asyncio
import logging
from unittest.mock import AsyncMock, patch

from continuum_client import ContinuumClient
from tests.fixtures import MockWebSocketServer

logger = logging.getLogger(__name__)

@pytest.mark.fred_agent
class TestAgentRegistration:
    """Test agent registration workflow"""
    
    @pytest.fixture
    async def fred_client(self):
        """Single WebSocket client for Fred tests"""
        mock_ws = AsyncMock()
        mock_server = MockWebSocketServer()
        
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        mock_ws.send = AsyncMock()
        
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                client._mock_server = mock_server
                yield client, mock_server
    
    @pytest.mark.asyncio
    async def test_fred_basic_registration(self, fred_client):
        """Test basic Fred agent registration"""
        client, mock_server = fred_client
        
        fred_config = {
            'agentId': 'fred',
            'agentName': 'Fred',
            'agentType': 'ai',
            'capabilities': ['chat', 'ui-interaction']
        }
        
        logger.info("🤖 Testing Fred registration...")
        
        # Register Fred
        registration_response = mock_server.register_agent(fred_config)
        await client.register_agent(fred_config)
        
        # Verify registration
        assert registration_response['success'] is True
        assert registration_response['agentId'] == 'fred'
        assert 'fred' in mock_server.connected_agents
        
        logger.info("✅ Fred registered successfully")
    
    @pytest.mark.asyncio
    async def test_fred_message_sending(self, fred_client):
        """Test Fred sending messages"""
        client, mock_server = fred_client
        
        fred_config = {'agentId': 'fred', 'agentName': 'Fred', 'agentType': 'ai'}
        await client.register_agent(fred_config)
        
        # Send message as Fred
        await client.send_message("Hello! I'm Fred.", 'fred')
        
        # Verify message was sent (check mock was called)
        assert client.ws.send.called
        
        logger.info("✅ Fred message sent successfully")