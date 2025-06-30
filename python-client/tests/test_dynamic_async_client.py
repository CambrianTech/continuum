"""
Test Dynamic Async Continuum Client
Validates modular middle-out architecture with async/await patterns
"""

import pytest
import asyncio
import aiohttp
from unittest.mock import Mock, AsyncMock, patch
import json

from continuum_client.core.dynamic_client import DynamicContinuumClient

class TestDynamicContinuumClient:
    """Test the async dynamic client with modular validation"""
    
    @pytest.fixture
    async def mock_session(self):
        """Mock aiohttp session for testing"""
        session = Mock(spec=aiohttp.ClientSession)
        session.get = AsyncMock()
        session.post = AsyncMock()
        return session
    
    @pytest.fixture
    async def client(self):
        """Create test client"""
        return DynamicContinuumClient("http://localhost:9000")
    
    @pytest.fixture
    async def connected_client(self, client, mock_session):
        """Client connected to mock system"""
        client._session = mock_session
        client._connected = True
        return client

    @pytest.mark.asyncio
    async def test_connect_to_existing_system(self, client, mock_session):
        """Test connecting to running system"""
        # Mock successful health check
        mock_response = Mock()
        mock_response.status = 200
        mock_session.get.return_value.__aenter__.return_value = mock_response
        
        with patch.object(client, '_session', mock_session):
            result = await client.connect()
            
        assert result == client
        assert client._connected == True
        mock_session.get.assert_called_once()

    @pytest.mark.asyncio
    async def test_command_discovery(self, connected_client, mock_session):
        """Test dynamic command discovery"""
        # Mock command discovery response
        mock_commands = {
            "health": {"description": "System health check"},
            "projects-list": {"description": "List all projects"},
            "daemon-status": {"description": "Show daemon status"}
        }
        
        mock_response = Mock()
        mock_response.status = 200
        mock_response.json = AsyncMock(return_value={"commands": mock_commands})
        mock_session.get.return_value.__aenter__.return_value = mock_response
        
        commands = await connected_client._discover_commands_async()
        
        assert "health" in commands
        assert "projects-list" in commands
        assert commands["health"]["description"] == "System health check"

    @pytest.mark.asyncio
    async def test_dynamic_method_execution(self, connected_client, mock_session):
        """Test dynamic method creation and execution"""
        # Mock command execution
        mock_response = Mock()
        mock_response.status = 200
        mock_response.json = AsyncMock(return_value={
            "success": True,
            "data": {"status": "healthy"}
        })
        mock_session.post.return_value.__aenter__.return_value = mock_response
        
        # Mock command discovery
        connected_client._commands_cache = {"health": {"description": "Health check"}}
        
        # Test dynamic method access
        health_method = getattr(connected_client, "health")
        assert callable(health_method)
        
        # Test execution
        result = await health_method()
        assert result == {"status": "healthy"}

    @pytest.mark.asyncio
    async def test_snake_case_to_kebab_case_conversion(self, connected_client):
        """Test Python snake_case converts to command kebab-case"""
        # Mock commands with kebab-case names
        connected_client._commands_cache = {
            "projects-list": {"description": "List projects"},
            "daemon-status": {"description": "Daemon status"}
        }
        
        # Test snake_case attribute access
        projects_method = getattr(connected_client, "projects_list")
        daemon_method = getattr(connected_client, "daemon_status")
        
        assert callable(projects_method)
        assert callable(daemon_method)

    @pytest.mark.asyncio
    async def test_jtag_console_hooks(self, connected_client, mock_session):
        """Test JTAG console integration hooks"""
        # Mock console logs response
        mock_response = Mock()
        mock_response.status = 200
        mock_response.json = AsyncMock(return_value=[
            {"level": "info", "message": "System started"},
            {"level": "error", "message": "Connection failed"}
        ])
        mock_session.get.return_value.__aenter__.return_value = mock_response
        
        # Test console_logs hook
        console_logs = getattr(connected_client, "console_logs")
        logs = await console_logs()
        
        assert len(logs) == 2
        assert logs[0]["level"] == "info"

    @pytest.mark.asyncio
    async def test_jtag_browser_hooks(self, connected_client, mock_session):
        """Test JTAG browser integration hooks"""
        # Mock screenshot response
        mock_response = Mock()
        mock_response.status = 200
        mock_response.content = b"fake_screenshot_data"
        mock_session.get.return_value.__aenter__.return_value = mock_response
        
        # Test browser_screenshot hook
        screenshot_method = getattr(connected_client, "browser_screenshot")
        screenshot = await screenshot_method()
        
        assert screenshot == b"fake_screenshot_data"

    @pytest.mark.asyncio
    async def test_jtag_daemon_hooks(self, connected_client, mock_session):
        """Test JTAG daemon management hooks"""
        # Mock daemon status response
        mock_response = Mock()
        mock_response.status = 200
        mock_response.json = AsyncMock(return_value={
            "daemons": {
                "websocket": {"status": "running", "pid": 1234},
                "renderer": {"status": "running", "pid": 5678}
            }
        })
        mock_session.get.return_value.__aenter__.return_value = mock_response
        
        # Test daemon_status hook
        daemon_status = getattr(connected_client, "daemon_status")
        status = await daemon_status()
        
        assert "daemons" in status
        assert "websocket" in status["daemons"]

    @pytest.mark.asyncio
    async def test_event_stream_integration(self, connected_client):
        """Test async event streaming for real-time updates"""
        # Mock WebSocket connection
        mock_websocket = Mock()
        mock_websocket.__aiter__ = AsyncMock(return_value=iter([
            '{"type": "console", "data": {"level": "info", "message": "Test log"}}',
            '{"type": "daemon", "data": {"name": "renderer", "status": "restarted"}}'
        ]))
        
        with patch('websockets.connect', return_value=mock_websocket):
            # Test console streaming
            events = []
            async for event in connected_client.console_stream():
                events.append(event)
                if len(events) >= 2:
                    break
            
            assert len(events) == 2
            assert events[0]["type"] == "console"

    @pytest.mark.asyncio
    async def test_error_handling(self, connected_client, mock_session):
        """Test proper error handling in async operations"""
        # Mock failed API call
        mock_response = Mock()
        mock_response.status = 500
        mock_response.text = "Internal Server Error"
        mock_session.post.return_value.__aenter__.return_value = mock_response
        
        connected_client._commands_cache = {"failing-command": {}}
        
        # Test command execution error
        failing_method = getattr(connected_client, "failing_command")
        
        with pytest.raises(RuntimeError, match="API error 500"):
            await failing_method()

    @pytest.mark.asyncio
    async def test_modular_layer_separation(self, connected_client):
        """Test middle-out architecture layer separation"""
        # Validate that different concern layers are properly separated
        
        # Layer 1: Core connection (lowest level)
        assert hasattr(connected_client, '_session')
        assert hasattr(connected_client, '_connected')
        
        # Layer 2: Command discovery (middle level)
        assert hasattr(connected_client, '_discover_commands_async')
        assert hasattr(connected_client, '_commands_cache')
        
        # Layer 3: Dynamic method creation (higher level)
        assert hasattr(connected_client, '__getattr__')
        
        # Layer 4: JTAG hooks (highest level, most specialized)
        assert hasattr(connected_client, '_get_console_hook')
        assert hasattr(connected_client, '_get_browser_hook')
        assert hasattr(connected_client, '_get_daemon_hook')

    def test_sync_wrapper_integration(self, client):
        """Test sync wrapper for compatibility"""
        with patch.object(client, 'connect') as mock_connect:
            mock_connect.return_value = asyncio.Future()
            mock_connect.return_value.set_result(client)
            
            # Test sync wrapper
            result = client.connect_sync()
            assert result == client

    @pytest.mark.asyncio
    async def test_resource_cleanup(self, connected_client):
        """Test proper resource cleanup"""
        # Mock session close
        connected_client._session.close = AsyncMock()
        
        await connected_client.close()
        
        connected_client._session.close.assert_called_once()

class TestMiddleOutArchitecture:
    """Test modular middle-out architecture principles"""
    
    def test_layer_independence(self):
        """Test that layers can work independently"""
        client = DynamicContinuumClient()
        
        # Layer 1: Connection layer works without commands
        assert client.base_url == "http://localhost:9000"
        assert not client._connected
        
        # Layer 2: Command discovery works independently
        client._commands_cache = {"test": {}}
        commands = client._commands_cache
        assert "test" in commands
        
        # Layer 3: Dynamic methods work with cached commands
        test_method = getattr(client, "test")
        assert callable(test_method)

    def test_composability(self):
        """Test that layers compose properly"""
        client = DynamicContinuumClient()
        
        # Each layer builds on the previous
        assert hasattr(client, 'connect')  # Layer 1
        assert hasattr(client, '_discover_commands_async')  # Layer 2  
        assert hasattr(client, '__getattr__')  # Layer 3
        assert hasattr(client, '_get_console_hook')  # Layer 4
        
        # Layers can be used in isolation for testing
        hook = client._get_console_hook('console_logs')
        assert callable(hook)

if __name__ == "__main__":
    pytest.main([__file__, "-v"])