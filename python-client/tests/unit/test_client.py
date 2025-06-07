"""
Integration tests for ContinuumClient 
Tests real WebSocket connection and Promise Post Office System
"""

import pytest
import asyncio
import json
import logging

from continuum_client import ContinuumClient, ContinuumServerManager
from continuum_client.exceptions.js_errors import ConnectionError

logger = logging.getLogger(__name__)

@pytest.mark.integration  
class TestContinuumClient:
    """Test real Continuum client against live server"""
    
    @pytest.fixture(scope="class")
    def continuum_server(self):
        """Start real Continuum server for testing"""
        with ContinuumServerManager() as server:
            yield server
    
    @pytest.mark.asyncio
    async def test_real_connection(self, continuum_server):
        """Test real WebSocket connection to Continuum server"""
        logger.info("🔌 Testing real connection...")
        
        async with ContinuumClient() as client:
            assert client.connected is True
            assert client.ws is not None
            assert client.js is not None
            logger.info("✅ Real connection successful")
    
    @pytest.mark.asyncio
    async def test_connection_to_nonexistent_server(self):
        """Test connection failure to non-existent server"""
        logger.info("❌ Testing connection failure...")
        
        with pytest.raises(ConnectionError):
            async with ContinuumClient("ws://localhost:9999") as client:
                pass
        
        logger.info("✅ Connection failure handled correctly")
    
    @pytest.mark.asyncio
    async def test_agent_registration(self, continuum_server):
        """Test real agent registration with Post Office System"""
        logger.info("🤖 Testing agent registration...")
        
        agent_info = {
            'agentId': 'test-agent',
            'agentName': 'Test Agent',
            'agentType': 'ai',
            'capabilities': ['testing', 'promise-post-office']
        }
        
        async with ContinuumClient() as client:
            await client.register_agent(agent_info)
            logger.info("✅ Agent registered successfully")
    
    @pytest.mark.asyncio
    async def test_send_message(self, continuum_server):
        """Test sending real chat message"""
        logger.info("💬 Testing message sending...")
        
        agent_info = {'agentId': 'test-sender', 'agentName': 'Test Sender', 'agentType': 'ai'}
        
        async with ContinuumClient() as client:
            await client.register_agent(agent_info)
            await client.send_message("Hello from Promise Post Office!", "test-sender", "auto")
            logger.info("✅ Message sent successfully")
    
    @pytest.mark.asyncio
    async def test_promise_post_office_flow(self, continuum_server):
        """Test complete Promise Post Office System flow"""
        logger.info("📮 Testing Promise Post Office flow...")
        
        async with ContinuumClient() as client:
            # Register as test agent  
            await client.register_agent({
                'agentId': 'post-office-tester',
                'agentName': 'Post Office Tester',
                'agentType': 'ai'
            })
            
            # Test 1: Simple promise resolution
            logger.info("  📤 Sending JS for promise resolution...")
            result = await client.js.get_value("return 'Hello from Promise Post Office!'")
            assert result == 'Hello from Promise Post Office!'
            logger.info("  ✅ Promise resolved successfully")
            
            # Test 2: Math calculation 
            logger.info("  📤 Sending math calculation...")
            result = await client.js.get_value("return 2 + 2")
            assert result == 4
            logger.info("  ✅ Math calculation resolved")
            
            # Test 3: DOM query (should work in browser context)
            logger.info("  📤 Testing DOM query...")
            title = await client.js.get_value("return document.title || 'Continuum Test'")
            assert isinstance(title, str)
            logger.info(f"  ✅ DOM query resolved: {title}")
            
            logger.info("✅ Promise Post Office flow complete!")
    
    def test_client_initialization(self):
        """Test client initialization"""
        client = ContinuumClient("ws://test:8080", timeout=15.0)
        
        assert client.url == "ws://test:8080"
        assert client.timeout == 15.0
        assert client.connected is False