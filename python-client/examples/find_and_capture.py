#!/usr/bin/env python3
"""
Find and Capture Example

This example shows how to search for specific elements and capture them.
Perfect for capturing UI components like sidebars, agent lists, etc.
"""

import asyncio
from screenshot_capture import ScreenshotCapture

async def find_and_capture_agents():
    """Find and capture the agents section specifically"""
    async with ScreenshotCapture() as capture:
        print("🔍 Searching for agents section...")
        
        # Try multiple search terms to find agents section
        search_terms = ['agents', 'user', 'sidebar', 'agent-list', 'agent-item']
        
        for term in search_terms:
            print(f"\n🎯 Searching for: '{term}'")
            
            find_result = await capture.find_element(term)
            if find_result['found']:
                print(f"✅ Found element: {find_result['selector']}")
                print(f"   Size: {find_result['width']}x{find_result['height']}")
                print(f"   Tag: {find_result['tag']}")
                
                # Capture this element
                result = await capture.capture(
                    selector=find_result['selector'],
                    format='png',
                    save_path=f'screenshots/{term}_section.png',
                    open_image=True
                )
                
                print(f"📸 Captured {term} section!")
                print(f"   Screenshot: {result['width']}x{result['height']}")
                print(f"   Saved to: {result.get('saved_path', 'temp file')}")
                
                # Only capture the first match
                break
            else:
                print(f"❌ No element found for '{term}'")
        
        print("\n🔍 Now trying direct selectors...")
        
        # Try common UI selectors
        selectors = [
            '#sidebar',
            '#agents',
            '.sidebar',
            '.agent-list',
            '.user-list',
            '[class*="agent"]',
            '[id*="agent"]'
        ]
        
        for selector in selectors:
            try:
                result = await capture.capture(
                    selector=selector,
                    format='png',
                    open_image=False  # Don't open every one
                )
                print(f"✅ Captured {selector}: {result['width']}x{result['height']}")
                break
            except Exception as e:
                print(f"❌ Failed {selector}: {e}")

async def main():
    """Main demo function"""
    print("🔍📸 Find and Capture Demo")
    print("=" * 30)
    
    await find_and_capture_agents()

if __name__ == "__main__":
    asyncio.run(main())