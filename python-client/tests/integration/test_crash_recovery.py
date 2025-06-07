"""
Test server restart and crash recovery
"""
import pytest
import asyncio
import logging
from unittest.mock import AsyncMock, patch

from continuum_client import ContinuumClient
from continuum_client.exceptions.js_errors import ConnectionError
from tests.fixtures import MockWebSocketServer

logger = logging.getLogger(__name__)

@pytest.mark.crash_recovery
class TestCrashRecovery:
    """Test server crash and recovery scenarios"""
    
    @pytest.fixture
    async def recovery_client(self):
        """WebSocket client for crash recovery testing"""
        mock_ws = AsyncMock()
        mock_server = MockWebSocketServer()
        
        mock_ws.recv = AsyncMock(side_effect=["status", "banner"])
        mock_ws.send = AsyncMock()
        
        with patch('websockets.connect', return_value=mock_ws):
            async with ContinuumClient() as client:
                client._mock_server = mock_server
                yield client, mock_server
    
    @pytest.mark.asyncio
    async def test_server_crash_simulation(self, recovery_client):
        """Test server crash during operations"""
        client, mock_server = recovery_client
        
        logger.info("🔄 Testing server crash...")
        
        # Register Fred initially
        fred_config = {'agentId': 'fred-crash', 'agentName': 'Fred Crash Test', 'agentType': 'ai'}
        await client.register_agent(fred_config)
        assert 'fred-crash' in mock_server.connected_agents
        
        # Simulate server crash
        mock_server.simulate_crash()
        
        # Attempt operation during crash (should fail)
        with pytest.raises(ConnectionError):
            mock_server.execute_js('return "test"', 'crash_test')
        
        logger.info("💥 Server crash simulated")
    
    @pytest.mark.asyncio
    async def test_server_restart_recovery(self, recovery_client):
        """Test server restart and agent recovery"""
        client, mock_server = recovery_client
        
        # Initial setup
        fred_config = {'agentId': 'fred-recovery', 'agentName': 'Fred Recovery', 'agentType': 'ai'}
        await client.register_agent(fred_config)
        
        # Crash and restart
        mock_server.simulate_crash()
        mock_server.simulate_restart()
        
        assert mock_server.restart_count == 1
        assert not mock_server.crash_simulation
        
        # Re-register after restart
        registration_response = mock_server.register_agent(fred_config)
        await client.register_agent(fred_config)
        
        assert registration_response['success'] is True
        assert 'fred-recovery' in mock_server.connected_agents
        
        logger.info("✅ Server restart recovery successful")