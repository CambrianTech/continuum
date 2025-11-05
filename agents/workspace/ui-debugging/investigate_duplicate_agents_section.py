#!/usr/bin/env python3
"""
Investigate duplicate USERS & AGENTS sections in left sidebar
Now that we have a working connection, let's debug the duplicate rendering
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

async def investigate_duplicate_agents():
    """Investigate why there are two USERS & AGENTS sections"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'duplicate-agents-investigator',
            'agentName': 'Duplicate Agents Section Investigator',
            'agentType': 'ai'
        })
        
        print("🔍 INVESTIGATING DUPLICATE USERS & AGENTS SECTIONS")
        print("=" * 55)
        
        # Detailed analysis of agent selector elements
        duplicate_analysis_js = """
        return new Promise((resolve) => {
            console.log('🔍 Analyzing duplicate USERS & AGENTS sections...');
            
            // Find all potential agent-related elements
            const agentSelectors = document.querySelectorAll('agent-selector, .agent-selector');
            const agentSections = document.querySelectorAll('[class*="agent"], [id*="agent"]');
            const usersAgentsText = Array.from(document.querySelectorAll('*')).filter(el => 
                el.textContent && el.textContent.includes('USERS') && el.textContent.includes('AGENTS')
            );
            
            // Analyze each agent selector
            const selectorAnalysis = Array.from(agentSelectors).map((el, index) => {
                const rect = el.getBoundingClientRect();
                const parent = el.parentElement;
                const siblings = parent ? Array.from(parent.children).indexOf(el) : -1;
                
                return {
                    index: index,
                    tagName: el.tagName.toLowerCase(),
                    className: el.className,
                    id: el.id,
                    position: {
                        top: Math.round(rect.top),
                        left: Math.round(rect.left),
                        width: Math.round(rect.width),
                        height: Math.round(rect.height)
                    },
                    visible: rect.width > 0 && rect.height > 0,
                    parentTag: parent ? parent.tagName.toLowerCase() : null,
                    parentClass: parent ? parent.className : null,
                    siblingIndex: siblings,
                    innerHTML: el.innerHTML.substring(0, 200) + '...',
                    hasUsersAgentsText: el.textContent.includes('USERS') && el.textContent.includes('AGENTS')
                };
            });
            
            // Find elements containing "USERS & AGENTS" text
            const usersAgentsElements = usersAgentsText.map((el, index) => {
                const rect = el.getBoundingClientRect();
                return {
                    index: index,
                    tagName: el.tagName.toLowerCase(),
                    className: el.className,
                    id: el.id,
                    position: {
                        top: Math.round(rect.top),
                        left: Math.round(rect.left),
                        width: Math.round(rect.width),
                        height: Math.round(rect.height)
                    },
                    textContent: el.textContent.substring(0, 100),
                    outerHTML: el.outerHTML.substring(0, 300) + '...'
                };
            });
            
            // Check sidebar structure
            const sidebar = document.querySelector('.sidebar');
            let sidebarStructure = null;
            if (sidebar) {
                sidebarStructure = {
                    childCount: sidebar.children.length,
                    children: Array.from(sidebar.children).map((child, index) => ({
                        index: index,
                        tagName: child.tagName.toLowerCase(),
                        className: child.className,
                        id: child.id,
                        hasAgentSelector: child.querySelector('agent-selector, .agent-selector') !== null,
                        textPreview: child.textContent.substring(0, 50).replace(/\\s+/g, ' ').trim()
                    }))
                };
            }
            
            // Check for script tags that might be loading duplicates
            const scriptTags = Array.from(document.querySelectorAll('script[src]')).map(script => ({
                src: script.src,
                loaded: script.readyState === 'complete' || !script.readyState
            }));
            
            resolve(JSON.stringify({
                success: true,
                agentSelectorCount: agentSelectors.length,
                selectorAnalysis: selectorAnalysis,
                usersAgentsTextElements: usersAgentsElements,
                sidebarStructure: sidebarStructure,
                scriptTags: scriptTags.filter(s => s.src.includes('AgentSelector')),
                timestamp: Date.now()
            }));
        });
        """
        
        print("🔍 Analyzing DOM structure for duplicate elements...")
        result = await client.js.get_value(duplicate_analysis_js, timeout=15)
        data = json.loads(result)
        
        if data.get('success'):
            selector_count = data.get('agentSelectorCount', 0)
            selector_analysis = data.get('selectorAnalysis', [])
            users_agents_elements = data.get('usersAgentsTextElements', [])
            sidebar_structure = data.get('sidebarStructure', {})
            script_tags = data.get('scriptTags', [])
            
            print(f"\n📊 AGENT SELECTOR ANALYSIS:")
            print(f"   • Total AgentSelector elements: {selector_count}")
            
            if selector_count > 1:
                print(f"\n⚠️  MULTIPLE AGENT SELECTORS FOUND:")
                for analysis in selector_analysis:
                    print(f"   {analysis['index']+1}. <{analysis['tagName']}> class='{analysis['className']}'")
                    print(f"      Position: ({analysis['position']['left']}, {analysis['position']['top']})")
                    print(f"      Size: {analysis['position']['width']}x{analysis['position']['height']}")
                    print(f"      Parent: <{analysis['parentTag']}> class='{analysis['parentClass']}'")
                    print(f"      Visible: {analysis['visible']}")
                    print(f"      Has 'USERS & AGENTS': {analysis['hasUsersAgentsText']}")
                    print()
            
            print(f"\n📊 'USERS & AGENTS' TEXT ELEMENTS:")
            print(f"   • Found {len(users_agents_elements)} elements with this text")
            for element in users_agents_elements:
                print(f"   • <{element['tagName']}> at ({element['position']['left']}, {element['position']['top']})")
                print(f"     Text: {element['textContent']}")
                print()
            
            print(f"\n📊 SIDEBAR STRUCTURE:")
            if sidebar_structure:
                print(f"   • Sidebar has {sidebar_structure.get('childCount', 0)} children")
                children = sidebar_structure.get('children', [])
                for child in children:
                    has_agent = "✅" if child['hasAgentSelector'] else "❌"
                    print(f"   {child['index']+1}. <{child['tagName']}> {has_agent} AgentSelector")
                    print(f"      Class: {child['className']}")
                    print(f"      Preview: {child['textPreview']}")
                    print()
            
            print(f"\n📊 SCRIPT LOADING:")
            print(f"   • AgentSelector scripts loaded: {len(script_tags)}")
            for script in script_tags:
                status = "✅" if script['loaded'] else "❌"
                print(f"   {status} {script['src']}")
        
        return True

async def main():
    """Main function"""
    print("🔍 Duplicate Agents Section Investigator")
    print("=" * 40)
    print()
    
    try:
        await investigate_duplicate_agents()
        
        print(f"\n" + "=" * 55)
        print(f"📋 INVESTIGATION COMPLETE")
        print(f"=" * 55)
        print()
        print(f"🔧 Likely Causes of Duplicates:")
        print(f"   • Two separate AgentSelector components rendered")
        print(f"   • HTML template contains duplicate sections")
        print(f"   • Component registration happening twice")
        print(f"   • CSS causing visual duplication")
        print()
        print(f"💡 Solutions:")
        print(f"   • Remove duplicate HTML in UIGenerator template")
        print(f"   • Check for duplicate component registration")
        print(f"   • Verify WebComponentsIntegration not duplicating")
        print(f"   • Check CSS for absolute positioning issues")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print(f"🔧 Troubleshooting:")
        print(f"   • Ensure Continuum server is running")
        print(f"   • Check WebSocket connection")
        print(f"   • Verify browser has active Continuum tab")

if __name__ == "__main__":
    asyncio.run(main())