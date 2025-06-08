#!/usr/bin/env python3
"""
Test server HTML generation to debug why changes aren't taking effect
"""
import asyncio
import json
import sys
import urllib.request
import urllib.error
from pathlib import Path

async def test_server_html_generation():
    """Test server HTML generation and compare with expectations"""
    
    print("🔍 SERVER HTML GENERATION TEST")
    print("=" * 40)
    
    # Check server version
    try:
        with urllib.request.urlopen('http://localhost:9000/version') as response:
            server_version_data = json.loads(response.read().decode())
        server_version = server_version_data.get('version', 'unknown')
        print(f"🖥️  Server Version: {server_version}")
    except Exception as e:
        print(f"❌ Failed to get server version: {e}")
        return False
    
    # Get the HTML from server
    try:
        with urllib.request.urlopen('http://localhost:9000/') as response:
            html_content = response.read().decode()
        print(f"📄 HTML Content Length: {len(html_content)} characters")
    except Exception as e:
        print(f"❌ Failed to get HTML: {e}")
        return False
    
    # Search for key indicators
    print(f"\n🔍 ANALYZING HTML CONTENT:")
    
    # Check for version information
    if f"v{server_version}" in html_content:
        print(f"✅ Server version {server_version} found in HTML")
    else:
        print(f"❌ Server version {server_version} NOT found in HTML")
        
        # Look for any version numbers
        import re
        version_matches = re.findall(r'v?(\d+\.\d+\.\d+)', html_content)
        if version_matches:
            print(f"⚠️  Found these versions in HTML: {set(version_matches)}")
        else:
            print(f"❌ No version numbers found in HTML at all")
    
    # Check for debug comments
    debug_comments = [
        "DEBUG: Starting script tags",
        "DEBUG: AgentSelector loaded", 
        "DEBUG: ChatHeader loaded",
        "DEBUG: All scripts loaded"
    ]
    
    print(f"\n📋 DEBUG COMMENTS CHECK:")
    for comment in debug_comments:
        if comment in html_content:
            print(f"✅ Found: {comment}")
        else:
            print(f"❌ Missing: {comment}")
    
    # Check for script tags
    script_tags = [
        'src="/src/ui/components/AgentSelector.js"',
        'src="/src/ui/components/ChatHeader.js"',
        'src="/src/ui/components/ChatArea.js"',
        'src="/src/ui/components/RoomTabs.js"',
        'src="/src/ui/components/StatusPill.js"',
        'src="/src/ui/components/AcademySection.js"'
    ]
    
    print(f"\n📜 SCRIPT TAGS CHECK:")
    for script in script_tags:
        if script in html_content:
            print(f"✅ Found: {script}")
        else:
            print(f"❌ Missing: {script}")
    
    # Check for old script tags
    old_script_tags = [
        'src="/ui/components/AgentSelector.js"'
    ]
    
    print(f"\n📜 OLD SCRIPT TAGS CHECK:")
    for script in old_script_tags:
        if script in html_content:
            print(f"⚠️  Found old: {script}")
        else:
            print(f"✅ Old script not found: {script}")
    
    # Check where HTML gets cut off
    web_components_index = html_content.find("<!-- Web Components -->")
    head_close_index = html_content.find("</head>")
    
    print(f"\n📍 HTML STRUCTURE CHECK:")
    print(f"   Web Components comment at: {web_components_index}")
    print(f"   </head> tag at: {head_close_index}")
    
    if web_components_index > 0 and head_close_index > 0:
        if head_close_index > web_components_index:
            print(f"✅ Web Components section comes before </head>")
            section_content = html_content[web_components_index:head_close_index]
            script_count = section_content.count('<script src=')
            print(f"   Script tags in Web Components section: {script_count}")
        else:
            print(f"❌ Web Components section comes after </head> - this is wrong!")
    
    # Check for template literal issues
    if '${' in html_content:
        print(f"\n⚠️  WARNING: Unprocessed template literal found in HTML")
        unprocessed = [line.strip() for line in html_content.split('\n') if '${' in line]
        for line in unprocessed[:5]:  # Show first 5
            print(f"   {line}")
    else:
        print(f"\n✅ No unprocessed template literals found")
    
    # Final summary
    print(f"\n" + "=" * 40)
    print(f"📋 SUMMARY")
    print(f"=" * 40)
    
    issues_found = []
    if f"v{server_version}" not in html_content:
        issues_found.append("Version mismatch in HTML")
    if "DEBUG: All scripts loaded" not in html_content:
        issues_found.append("Debug comments missing - HTML generation incomplete")
    if 'src="/src/ui/components/ChatHeader.js"' not in html_content:
        issues_found.append("New script tags missing")
    if 'src="/ui/components/AgentSelector.js"' in html_content:
        issues_found.append("Old script tags still present")
    
    if issues_found:
        print(f"❌ Issues found:")
        for issue in issues_found:
            print(f"   • {issue}")
        print(f"\n💡 Likely causes:")
        print(f"   • UIGenerator require cache not cleared")
        print(f"   • Multiple UIGenerator instances")
        print(f"   • Template literal syntax errors")
        print(f"   • Server not restarting properly")
    else:
        print(f"✅ All checks passed - HTML generation looks correct")
    
    return len(issues_found) == 0

async def main():
    """Main function"""
    print("🔍 Server HTML Generation Tester")
    print("=" * 35)
    print()
    
    try:
        success = await test_server_html_generation()
        if not success:
            print(f"\n🔧 Next steps:")
            print(f"   • Check UIGenerator.cjs for syntax errors")
            print(f"   • Verify require cache clearing")
            print(f"   • Test manual HTML generation")
            print(f"   • Check for multiple server instances")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")

if __name__ == "__main__":
    asyncio.run(main())