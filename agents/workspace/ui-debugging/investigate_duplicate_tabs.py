#!/usr/bin/env python3
"""
Investigate duplicate browser tabs causing duplicate UI elements
"""
import asyncio
import json
import sys
from pathlib import Path

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent.parent.parent / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def investigate_duplicate_tabs():
    """Investigate duplicate tabs and their UI elements"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'tab-investigator',
            'agentName': 'Duplicate Tab Investigator',
            'agentType': 'ai'
        })
        
        print("🔍 INVESTIGATING DUPLICATE TABS AND UI ELEMENTS")
        print("=" * 50)
        
        # Check for duplicate components and tab info
        tab_investigation_js = """
        return new Promise((resolve) => {
            console.log('🔍 Investigating tabs and UI elements...');
            
            // Get tab information
            const tabInfo = {
                url: window.location.href,
                tabId: sessionStorage.getItem('continuum-tab-id'),
                timestamp: Date.now(),
                userAgent: navigator.userAgent,
                windowName: window.name
            };
            
            // Check for AgentSelector elements
            const agentSelectors = document.querySelectorAll('agent-selector, .agent-selector');
            const agentSelectorInfo = Array.from(agentSelectors).map((el, index) => {
                const rect = el.getBoundingClientRect();
                return {
                    index: index,
                    tagName: el.tagName.toLowerCase(),
                    className: el.className,
                    id: el.id,
                    innerHTML: el.innerHTML.substring(0, 100) + '...',
                    position: {
                        top: rect.top,
                        left: rect.left,
                        width: rect.width,
                        height: rect.height
                    },
                    visible: rect.width > 0 && rect.height > 0,
                    inViewport: rect.top >= 0 && rect.left >= 0 && 
                               rect.bottom <= window.innerHeight && 
                               rect.right <= window.innerWidth,
                    hasParent: el.parentElement !== null,
                    parentClass: el.parentElement ? el.parentElement.className : null
                };
            });
            
            // Check for other duplicate components
            const componentCounts = {
                'chat-header': document.querySelectorAll('chat-header, .chat-header').length,
                'chat-area': document.querySelectorAll('chat-area, .chat-area').length,
                'room-tabs': document.querySelectorAll('room-tabs, .room-tabs').length,
                'status-pill': document.querySelectorAll('status-pill, .status-pill').length,
                'academy-section': document.querySelectorAll('academy-section, .academy-section').length
            };
            
            // Check sidebar structure
            const sidebarElements = document.querySelectorAll('.sidebar');
            const sidebarInfo = Array.from(sidebarElements).map((el, index) => ({
                index: index,
                className: el.className,
                childCount: el.children.length,
                hasAgentSelector: el.querySelector('agent-selector, .agent-selector') !== null
            }));
            
            // Check for WebSocket connections
            let wsInfo = 'WebSocket not available';
            if (typeof ws !== 'undefined') {
                wsInfo = {
                    readyState: ws.readyState,
                    url: ws.url,
                    protocol: ws.protocol
                };
            }
            
            resolve(JSON.stringify({
                success: true,
                tabInfo: tabInfo,
                agentSelectorCount: agentSelectors.length,
                agentSelectorDetails: agentSelectorInfo,
                componentCounts: componentCounts,
                sidebarCount: sidebarElements.length,
                sidebarDetails: sidebarInfo,
                websocketInfo: wsInfo
            }));
        });
        """
        
        print("🔍 Analyzing current tab and UI elements...")
        result = await client.js.get_value(tab_investigation_js, timeout=15)
        data = json.loads(result)
        
        if data.get('success'):
            tab_info = data.get('tabInfo', {})
            agent_count = data.get('agentSelectorCount', 0)
            agent_details = data.get('agentSelectorDetails', [])
            component_counts = data.get('componentCounts', {})
            sidebar_count = data.get('sidebarCount', 0)
            sidebar_details = data.get('sidebarDetails', [])
            ws_info = data.get('websocketInfo', {})
            
            print(f"\n📊 TAB INFORMATION:")
            print(f"   • URL: {tab_info.get('url', 'Unknown')}")
            print(f"   • Tab ID: {tab_info.get('tabId', 'Unknown')}")
            print(f"   • Window Name: {tab_info.get('windowName', 'Unknown')}")
            
            print(f"\n🔍 AGENT SELECTOR ANALYSIS:")
            print(f"   • Total AgentSelector elements: {agent_count}")
            
            if agent_count > 1:
                print(f"⚠️  MULTIPLE AGENT SELECTORS DETECTED!")
                for detail in agent_details:
                    print(f"   {detail['index']+1}. <{detail['tagName']}> class='{detail['className']}'")
                    print(f"      Position: ({detail['position']['left']:.0f}, {detail['position']['top']:.0f})")
                    print(f"      Size: {detail['position']['width']:.0f}x{detail['position']['height']:.0f}")
                    print(f"      Visible: {detail['visible']}, In Viewport: {detail['inViewport']}")
                    print(f"      Parent: {detail['parentClass']}")
            elif agent_count == 1:
                print(f"✅ Single AgentSelector found")
                detail = agent_details[0]
                print(f"   <{detail['tagName']}> class='{detail['className']}'")
                print(f"   Position: ({detail['position']['left']:.0f}, {detail['position']['top']:.0f})")
                print(f"   Size: {detail['position']['width']:.0f}x{detail['position']['height']:.0f}")
            else:
                print(f"❌ No AgentSelector elements found")
            
            print(f"\n📊 OTHER COMPONENT COUNTS:")
            for component, count in component_counts.items():
                status = "✅" if count == 1 else "⚠️" if count > 1 else "❌"
                print(f"   {status} {component}: {count}")
            
            print(f"\n📊 SIDEBAR ANALYSIS:")
            print(f"   • Total sidebar elements: {sidebar_count}")
            for detail in sidebar_details:
                print(f"   {detail['index']+1}. Sidebar with {detail['childCount']} children")
                print(f"      Has AgentSelector: {detail['hasAgentSelector']}")
            
            print(f"\n🔌 WEBSOCKET CONNECTION:")
            if isinstance(ws_info, dict):
                print(f"   • Ready State: {ws_info.get('readyState', 'Unknown')}")
                print(f"   • URL: {ws_info.get('url', 'Unknown')}")
            else:
                print(f"   • Status: {ws_info}")
        
        return True

async def main():
    """Main function"""
    print("🔍 Duplicate Tab Investigator")
    print("=" * 30)
    print()
    
    try:
        await investigate_duplicate_tabs()
        
        print(f"\n" + "=" * 50)
        print(f"📋 INVESTIGATION COMPLETE")
        print(f"=" * 50)
        print()
        print(f"💡 Recommendations:")
        print(f"   • If multiple AgentSelectors found: Close extra browser tabs")
        print(f"   • Check for duplicate component registrations")
        print(f"   • Verify only one WebSocket connection is active")
        print(f"   • Use browser dev tools to inspect element hierarchy")
        print()
        print(f"🔧 Quick Fixes:")
        print(f"   • Close all Continuum tabs and restart with fresh tab")
        print(f"   • Clear browser cache and reload")
        print(f"   • Check for CSS z-index stacking issues")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print(f"🔧 Troubleshooting:")
        print(f"   • Ensure Continuum server is running")
        print(f"   • Check WebSocket connection")
        print(f"   • Verify browser has active Continuum tab")

if __name__ == "__main__":
    asyncio.run(main())