#!/usr/bin/env python3
"""
Simple Screenshot Example

Quick example showing how to capture screenshots with the Continuum Promise Post Office System.
This example captures the page and opens it automatically.
"""

import asyncio
from screenshot_capture import ScreenshotCapture

async def main():
    """Simple screenshot capture demo"""
    print("📸 Simple Screenshot Capture Demo")
    print("=" * 35)
    
    async with ScreenshotCapture() as capture:
        print("\n🎯 Capturing screenshot and opening it...")
        
        # Capture screenshot and open it automatically
        result = await capture.capture(
            selector='body',           # Capture full page
            format='png',              # PNG format
            open_image=True            # Open automatically
        )
        
        print(f"✅ Screenshot captured!")
        print(f"   Size: {result['width']}x{result['height']} pixels")
        print(f"   Format: {result['format'].upper()}")
        print(f"   Data: {len(result['dataURL'])} characters base64")
        
        if 'temp_path' in result:
            print(f"   File: {result['temp_path']}")
        
        print("\n🖼️ Image should now be open in your default image viewer!")

if __name__ == "__main__":
    asyncio.run(main())