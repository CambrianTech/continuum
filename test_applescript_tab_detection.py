#!/usr/bin/env python3
"""
Test AppleScript tab detection to debug why tabs aren't being found
"""
import subprocess
import json

def test_browser_tab_detection():
    """Test if AppleScript can find the Continuum tabs"""
    print("🔍 Testing AppleScript Browser Tab Detection")
    print("=" * 45)
    
    # Test Opera tab detection
    print("\n🟠 Testing Opera tab detection...")
    opera_script = '''
    tell application "Opera"
        set allTabs to {}
        repeat with w in windows
            repeat with t in tabs of w
                set end of allTabs to (URL of t)
            end repeat
        end repeat
        return allTabs as string
    end tell
    '''
    
    try:
        result = subprocess.run(['osascript', '-e', opera_script], 
                              capture_output=True, text=True, timeout=10)
        if result.returncode == 0:
            urls = result.stdout.strip()
            print(f"   📊 Opera URLs found: {urls}")
            if 'localhost:9000' in urls:
                print("   ✅ Found localhost:9000 in Opera")
            else:
                print("   ❌ localhost:9000 not found in Opera")
        else:
            print(f"   ❌ Opera error: {result.stderr}")
    except Exception as e:
        print(f"   ❌ Opera exception: {e}")
    
    # Test Chrome tab detection  
    print("\n🔵 Testing Chrome tab detection...")
    chrome_script = '''
    tell application "Google Chrome"
        set allTabs to {}
        repeat with w in windows
            repeat with t in tabs of w
                set end of allTabs to (URL of t)
            end repeat
        end repeat
        return allTabs as string
    end tell
    '''
    
    try:
        result = subprocess.run(['osascript', '-e', chrome_script], 
                              capture_output=True, text=True, timeout=10)
        if result.returncode == 0:
            urls = result.stdout.strip()
            print(f"   📊 Chrome URLs found: {urls}")
            if 'localhost:9000' in urls:
                print("   ✅ Found localhost:9000 in Chrome")
            else:
                print("   ❌ localhost:9000 not found in Chrome")
        else:
            print(f"   ❌ Chrome error: {result.stderr}")
    except Exception as e:
        print(f"   ❌ Chrome exception: {e}")
    
    # Test focused tab detection
    print("\n🎯 Testing specific localhost:9000 tab focus...")
    focus_script = '''
    tell application "Google Chrome"
        repeat with w in windows
            repeat with t in tabs of w
                if URL of t contains "localhost:9000" then
                    set active tab index of w to index of t
                    set index of w to 1
                    activate
                    return "focused tab: " & (URL of t)
                end if
            end repeat
        end repeat
        return "no localhost:9000 tab found"
    end tell
    '''
    
    try:
        result = subprocess.run(['osascript', '-e', focus_script], 
                              capture_output=True, text=True, timeout=10)
        if result.returncode == 0:
            output = result.stdout.strip()
            print(f"   📊 Focus result: {output}")
            if 'focused tab:' in output:
                print("   ✅ Successfully focused localhost:9000 tab")
            else:
                print("   ❌ Could not focus localhost:9000 tab")
        else:
            print(f"   ❌ Focus error: {result.stderr}")
    except Exception as e:
        print(f"   ❌ Focus exception: {e}")

def test_running_browsers():
    """Test which browsers are currently running"""
    print("\n🔍 Testing running browser detection...")
    
    script = '''tell application "System Events" to get name of every application process'''
    
    try:
        result = subprocess.run(['osascript', '-e', script], 
                              capture_output=True, text=True, timeout=10)
        if result.returncode == 0:
            processes = result.stdout.strip().split(', ')
            browsers = []
            
            browser_map = {
                'Opera': 'opera',
                'Google Chrome': 'chrome', 
                'Safari': 'safari',
                'Firefox': 'firefox'
            }
            
            for process_name, browser_id in browser_map.items():
                if process_name in processes:
                    browsers.append(browser_id)
            
            print(f"   📊 Running browsers: {browsers}")
            return browsers
        else:
            print(f"   ❌ Error detecting browsers: {result.stderr}")
            return []
    except Exception as e:
        print(f"   ❌ Exception detecting browsers: {e}")
        return []

if __name__ == "__main__":
    test_running_browsers()
    test_browser_tab_detection()
    
    print("\n" + "=" * 45)
    print("📋 DIAGNOSIS SUMMARY")
    print("=" * 45)
    print()
    print("🔧 This test helps debug why continuum --restart")
    print("   opens new tabs instead of focusing existing ones.")
    print()
    print("🎯 Expected behavior:")
    print("   • AppleScript should find localhost:9000 tabs")
    print("   • Focus commands should switch to existing tab")
    print("   • No new tabs should be opened")
    print()
    print("💡 If localhost:9000 tabs aren't found:")
    print("   • Check if the tab URL is exactly 'http://localhost:9000/'")
    print("   • Verify AppleScript permissions for the browser") 
    print("   • Test manual focus commands")
    print("   • Consider improving the domain matching logic")