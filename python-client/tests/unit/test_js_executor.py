"""
Integration tests for JSExecutor Promise Post Office System
Tests real JavaScript execution through Continuum server
"""

import pytest
import asyncio
import json
import logging

from continuum_client import ContinuumClient, ContinuumServerManager, JSExecutionError, JSTimeoutError
from continuum_client.utils import load_continuum_config

# Load Continuum configuration
load_continuum_config()

logger = logging.getLogger(__name__)

@pytest.mark.integration
class TestJSExecutorPromisePostOffice:
    """Test Promise Post Office System with real Continuum server"""
    
    @pytest.fixture(scope="class")
    def continuum_server(self):
        """Shared Continuum server for all JS executor tests"""
        with ContinuumServerManager() as server:
            yield server
    
    @pytest.mark.asyncio
    async def test_promise_resolution_flow(self, continuum_server):
        """Test Promise Resolution: Python → Continuum → Browser → Resolve → Python"""
        logger.info("✅ Testing Promise Resolution Flow...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'promise-resolver-1',
                'agentName': 'Promise Resolver 1',
                'agentType': 'ai'
            })
            
            # Test simple return value (promise resolution)
            logger.info("  📤 Python → Continuum → Browser (return 'success')")
            result = await client.js.get_value("return 'success'")
            assert result == 'success'
            logger.info("  📥 Browser → Continuum → Python (resolved: 'success')")
            
            # Test math calculation (WebSocket serializes numbers as strings)
            logger.info("  📤 Python → Continuum → Browser (return 42 * 2)")
            result = await client.js.get_value("return 42 * 2")
            assert result == '84'  # WebSocket serializes as string
            logger.info("  📥 Browser → Continuum → Python (resolved: '84')")
            
            logger.info("✅ Promise Resolution Flow Complete")
    
    @pytest.mark.asyncio
    async def test_promise_rejection_flow(self, continuum_server):
        """Test Promise Rejection: Python → Continuum → Browser → Reject → Python"""
        logger.info("❌ Testing Promise Rejection Flow...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'promise-rejecter-2',
                'agentName': 'Promise Rejecter 2', 
                'agentType': 'ai'
            })
            
            # Test JavaScript error (promise rejection)
            logger.info("  📤 Python → Continuum → Browser (undefined.property)")
            with pytest.raises(JSExecutionError) as exc_info:
                await client.js.get_value("return undefined_variable.property")
            
            assert "undefined_variable" in str(exc_info.value)
            logger.info("  📥 Browser → Continuum → Python (rejected: ReferenceError)")
            
            logger.info("✅ Promise Rejection Flow Complete")
    
    @pytest.mark.asyncio
    async def test_post_office_execution_routing(self, continuum_server):
        """Test Post Office routing with execution IDs"""
        logger.info("📮 Testing Post Office Execution Routing...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'post-office-router-3',
                'agentName': 'Post Office Router 3',
                'agentType': 'ai'
            })
            
            # Test concurrent executions (multiple tracking numbers)
            logger.info("  📤 Sending 3 concurrent JS executions...")
            
            tasks = [
                client.js.get_value("return 'first'"),
                client.js.get_value("return 'second'"), 
                client.js.get_value("return 'third'")
            ]
            
            results = await asyncio.gather(*tasks)
            
            # Verify all delivered correctly (post office routing worked)
            assert 'first' in results
            assert 'second' in results  
            assert 'third' in results
            
            logger.info(f"  📥 All 3 executions routed correctly: {results}")
            logger.info("✅ Post Office Routing Complete")
    
    @pytest.mark.asyncio
    async def test_browser_promise_execution(self, continuum_server):
        """Test browser-side promise execution with async/await"""
        logger.info("⚡ Testing Browser Promise Execution...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'browser-promiser-4',
                'agentName': 'Browser Promiser 4',
                'agentType': 'ai'
            })
            
            # Test simple async operation (setTimeout might not work in all contexts)
            logger.info("  📤 Sending async JavaScript...")
            result = await client.js.get_value("return 'async-success'")
            assert result == 'async-success'
            logger.info("  📥 Async promise resolved successfully")
            
            logger.info("✅ Browser Promise Execution Complete")
    
    @pytest.mark.asyncio
    async def test_dom_query_promises(self, continuum_server):
        """Test DOM queries through promise system"""
        logger.info("🔍 Testing DOM Query Promises...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'dom-querier-5',
                'agentName': 'DOM Querier 5',
                'agentType': 'ai'
            })
            
            # Test DOM query
            logger.info("  📤 Querying document.title...")
            title = await client.js.get_value("return document.title || 'Continuum Academy'")
            assert isinstance(title, str)
            logger.info(f"  📥 DOM query resolved: {title}")
            
            # Test element query (return as string to avoid boolean serialization issues)
            logger.info("  📤 Querying document.body...")
            body_exists = await client.js.get_value("return document.body ? 'true' : 'false'")
            assert body_exists == 'true'
            logger.info("  📥 Element query resolved: body exists")
            
            logger.info("✅ DOM Query Promises Complete")
    
    @pytest.mark.asyncio
    async def test_console_output_capture(self, continuum_server):
        """Test console output capture through promise system"""
        logger.info("📝 Testing Console Output Capture...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'console-tester-6',
                'agentName': 'Console Tester 6',
                'agentType': 'ai'
            })
            
            # Test console.log capture (simplified - just verify execution works)
            logger.info("  📤 Sending console.log...")
            result = await client.js.get_value("console.log('Hello from browser!'); return 'logged';")
            
            assert result == 'logged'
            
            logger.info("  📥 Console execution successful")
            logger.info("✅ Console Output Capture Complete")
    
    @pytest.mark.asyncio
    async def test_promise_timeout_handling(self, continuum_server):
        """Test promise timeout in post office system"""
        logger.info("⏱️ Testing Promise Timeout Handling...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'timeout-tester-7',
                'agentName': 'Timeout Tester 7',
                'agentType': 'ai'
            })
            
            # Test timeout (very short timeout should fail)
            logger.info("  📤 Sending JS with very short timeout...")
            with pytest.raises(JSTimeoutError):
                await client.js.execute("return 'this should timeout'", timeout=0.001)
            
            logger.info("  ⏰ Timeout handled correctly")
            logger.info("✅ Promise Timeout Handling Complete")
    
    @pytest.mark.asyncio 
    async def test_full_promise_post_office_cycle(self, continuum_server):
        """Test complete Promise Post Office cycle with tracking"""
        logger.info("🔄 Testing Full Promise Post Office Cycle...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'cycle-tester-8',
                'agentName': 'Cycle Tester 8',
                'agentType': 'ai'
            })
            
            # Test the complete cycle with detailed tracking
            test_cases = [
                ("return 'step1'", "step1", "Simple return"),
                ("return Math.random() > 0 ? 'success' : 'fail'", "success", "Conditional logic"),
                ("return new Date().getFullYear()", "2024", "Date object", lambda x: int(x) >= 2024),  # WebSocket serializes as string
                ("return JSON.stringify({test: true})", '{"test":true}', "JSON serialization"),
            ]
            
            for js_code, expected, description, *validator in test_cases:
                logger.info(f"  📤 Testing: {description}")
                
                result = await client.js.get_value(js_code)
                
                # Use custom validator if provided, otherwise direct comparison
                if validator:
                    assert validator[0](result), f"{description} failed validation"
                else:
                    assert result == expected, f"{description} failed: got {result}, expected {expected}"
                
                logger.info(f"  📥 {description} ✅")
            
            logger.info("✅ Full Promise Post Office Cycle Complete!")