#!/usr/bin/env python3
"""
Simple screenshot using working validation system
"""
import asyncio
import subprocess
import time

def take_screenshot():
    print("📸 Taking screenshot using working validation system...")
    
    # Run the validation which includes screenshot
    result = subprocess.run([
        'python3', 'python-client/fix_browser_validation.py'
    ], capture_output=True, text=True)
    
    # Check if screenshot was successful
    if "Screenshot SUCCESS" in result.stdout:
        print("✅ Screenshot taken successfully!")
        
        # Extract dimensions and data length
        lines = result.stdout.split('\n')
        for line in lines:
            if "Screenshot SUCCESS" in line:
                print(f"📏 {line}")
            elif "DataURL length" in line:
                print(f"💾 {line}")
                
        print("\n🎉 Screenshot captured as part of validation process!")
        print("📋 The screenshot data is processed through the BrowserClientConnection")
        print("🔄 Using promises and asyncio for proper event handling")
        
        return True
    else:
        print("❌ Screenshot failed")
        if result.stdout:
            print("Output:", result.stdout[-500:])  # Last 500 chars
        if result.stderr:
            print("Error:", result.stderr[-500:])
        return False

if __name__ == "__main__":
    success = take_screenshot()
    print(f"\n🎯 FINAL RESULT: {'SUCCESS' if success else 'FAILED'}")