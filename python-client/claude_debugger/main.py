"""
Claude Unified Debugger - Main Orchestrator
===========================================

Main class that coordinates all components for comprehensive debugging.
"""

import asyncio
from continuum_client.utils import get_continuum_ws_url, load_continuum_config

from .connection import WebSocketConnection
from .validation import ConnectionValidator
from .managers import ScreenshotManager


class ClaudeDebugger:
    """
    Main Debugger Orchestrator
    
    Coordinates all components for comprehensive Continuum validation
    and debugging capabilities. Provides self-validating connection
    that works with browser client validation through server logs.
    
    Components:
    - WebSocketConnection: Core connection management
    - ConnectionValidator: Diagnostic validation system
    - ScreenshotManager: Screenshot capture and management
    
    Features:
    - Self-validating connection with diagnostic checks
    - Cross-client validation coordination
    - Complete UI debugging capabilities
    - Modular component architecture
    """
    
    def __init__(self):
        load_continuum_config()
        self.ws_url = get_continuum_ws_url()
        self.connection = None
        self.validator = None
        self.screenshot_manager = None
        
    async def initialize(self):
        """Initialize all components"""
        self.connection = WebSocketConnection(self.ws_url)
        await self.connection.connect()
        
        self.validator = ConnectionValidator(self.connection)
        self.screenshot_manager = ScreenshotManager(self.connection)
        
    async def validate_connection(self):
        """Run full connection validation"""
        if not self.connection or not self.connection.is_connected:
            print(f"❌ Not connected to WebSocket")
            return False
            
        return await self.validator.validate_all()
        
    async def test_screenshot_capability(self):
        """Test screenshot functionality"""
        if not self.screenshot_manager:
            return False
        return await self.screenshot_manager.test_screenshot()
        
    async def capture_ui_element(self, selector):
        """Capture screenshot of specific UI element"""
        if not self.screenshot_manager:
            return False
        return await self.screenshot_manager.capture_element_screenshot(selector)
        
    def get_validation_results(self):
        """Get validation results for cross-client sharing"""
        if self.validator:
            return self.validator.validation_results
        return {}
        
    async def cleanup(self):
        """Clean up all connections and resources"""
        if self.connection:
            await self.connection.disconnect()


async def main():
    """Main entry point for Claude unified debugger"""
    debugger = ClaudeDebugger()
    
    try:
        # Initialize all components
        await debugger.initialize()
        print(f"🔌 Claude connected to Continuum: {debugger.ws_url}")
        
        # Run self-validation
        validation_success = await debugger.validate_connection()
        
        if validation_success:
            print(f"\n🔧 CLAUDE DEBUGGER: OPERATIONAL")
            print(f"✅ Connection self-validated successfully")
            print(f"✅ Ready for cross-client validation and UI debugging")
            
            # Show validation results for cross-client reference
            results = debugger.get_validation_results()
            print(f"\n📋 Validation results available for other clients:")
            for check, result in results.items():
                print(f"   {check}: {'✅' if result else '❌'}")
                
            # Test screenshot capability if connection is good
            print(f"\n📸 Testing screenshot capability...")
            screenshot_success = await debugger.test_screenshot_capability()
            if screenshot_success:
                print(f"✅ Screenshot capability confirmed")
            else:
                print(f"⚠️ Screenshot capability needs work")
                
        else:
            print(f"\n🔧 CLAUDE DEBUGGER: NEEDS WORK") 
            print(f"❌ Connection validation failed")
            print(f"💡 Check browser connection and Continuum server status")
            
    except Exception as e:
        print(f"❌ Debugger initialization failed: {e}")
        
    finally:
        # Clean up connections
        await debugger.cleanup()


if __name__ == "__main__":
    asyncio.run(main())