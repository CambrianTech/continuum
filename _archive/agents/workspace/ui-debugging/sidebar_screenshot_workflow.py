#!/usr/bin/env python3
"""
Left Sidebar Screenshot Workflow
Reusable tool for debugging sidebar issues with visual feedback
"""
import asyncio
import json
import base64
import tempfile
import subprocess
import sys
from pathlib import Path
from datetime import datetime

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent.parent.parent / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def capture_sidebar_debug():
    """Capture sidebar with detailed analysis"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'sidebar-debug-workflow',
            'agentName': 'Sidebar Debug Workflow',
            'agentType': 'ai'
        })
        
        print("📸 SIDEBAR SCREENSHOT WORKFLOW")
        print("=" * 40)
        
        # 1. Analyze sidebar structure first
        analysis_js = """
        return new Promise((resolve) => {
            const sidebar = document.querySelector('.sidebar');
            const agentSelectors = document.querySelectorAll('agent-selector');
            const agentDivs = document.querySelectorAll('.agent-selector');
            const usersAgentsText = Array.from(document.querySelectorAll('*')).filter(el => 
                el.textContent && el.textContent.includes('USERS') && el.textContent.includes('AGENTS')
            );
            
            let sidebarChildren = [];
            if (sidebar) {
                sidebarChildren = Array.from(sidebar.children).map((child, index) => ({
                    index: index,
                    tagName: child.tagName.toLowerCase(),
                    className: child.className,
                    id: child.id,
                    textPreview: child.textContent.substring(0, 50).replace(/\\s+/g, ' ').trim(),
                    hasAgentSelector: child.querySelector('agent-selector, .agent-selector') !== null
                }));
            }
            
            resolve(JSON.stringify({
                sidebarExists: !!sidebar,
                sidebarRect: sidebar ? sidebar.getBoundingClientRect() : null,
                agentSelectorCount: agentSelectors.length,
                agentDivCount: agentDivs.length,
                usersAgentsTextCount: usersAgentsText.length,
                sidebarChildren: sidebarChildren,
                timestamp: Date.now()
            }));
        });
        """
        
        print("🔍 Analyzing sidebar structure...")
        analysis_result = await client.js.get_value(analysis_js, timeout=10)
        analysis_data = json.loads(analysis_result)
        
        print(f"   Sidebar exists: {analysis_data['sidebarExists']}")
        print(f"   AgentSelector elements: {analysis_data['agentSelectorCount']}")
        print(f"   Legacy agent divs: {analysis_data['agentDivCount']}")
        print(f"   'USERS & AGENTS' text elements: {analysis_data['usersAgentsTextCount']}")
        print(f"   Sidebar children: {len(analysis_data['sidebarChildren'])}")
        
        for child in analysis_data['sidebarChildren']:
            indicator = "✅" if child['hasAgentSelector'] else "❌"
            print(f"      {child['index']+1}. <{child['tagName']}> {indicator} - {child['textPreview']}")
        
        # 2. Take sidebar screenshot
        screenshot_js = """
        return new Promise((resolve) => {
            if (typeof html2canvas === 'undefined') {
                const script = document.createElement('script');
                script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                script.onload = () => takeScreenshot();
                script.onerror = () => resolve(JSON.stringify({success: false, error: 'Failed to load html2canvas'}));
                document.head.appendChild(script);
            } else {
                takeScreenshot();
            }
            
            function takeScreenshot() {
                const sidebar = document.querySelector('.sidebar');
                if (!sidebar) {
                    resolve(JSON.stringify({success: false, error: 'Sidebar not found'}));
                    return;
                }
                
                html2canvas(sidebar, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 1,
                    backgroundColor: null
                }).then(canvas => {
                    const dataURL = canvas.toDataURL('image/png');
                    resolve(JSON.stringify({
                        success: true,
                        dataURL: dataURL,
                        width: canvas.width,
                        height: canvas.height,
                        timestamp: Date.now()
                    }));
                }).catch(error => {
                    resolve(JSON.stringify({success: false, error: error.message}));
                });
            }
        });
        """
        
        print("\n📸 Capturing sidebar screenshot...")
        screenshot_result = await client.js.get_value(screenshot_js, timeout=30)
        screenshot_data = json.loads(screenshot_result)
        
        if screenshot_data['success']:
            # Save screenshot with timestamp
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            base64_data = screenshot_data['dataURL'].split(',')[1]
            image_bytes = base64.b64decode(base64_data)
            
            screenshot_path = f"/Users/joel/.continuum/screenshots/sidebar_debug_{timestamp}.png"
            Path(screenshot_path).parent.mkdir(parents=True, exist_ok=True)
            
            with open(screenshot_path, 'wb') as f:
                f.write(image_bytes)
            
            print(f"✅ Screenshot saved: {screenshot_path}")
            print(f"   Size: {screenshot_data['width']}x{screenshot_data['height']}")
            
            # Open screenshot
            subprocess.run(['open', screenshot_path])
            print("🖼️  Screenshot opened for visual inspection")
            
            return {
                'analysis': analysis_data,
                'screenshot_path': screenshot_path,
                'screenshot_size': f"{screenshot_data['width']}x{screenshot_data['height']}"
            }
        else:
            print(f"❌ Screenshot failed: {screenshot_data['error']}")
            return {'analysis': analysis_data, 'screenshot_path': None}

async def main():
    """Run the sidebar debug workflow"""
    print("🚀 Starting Sidebar Debug Workflow")
    print("🎯 Purpose: Capture and analyze left sidebar for debugging")
    print()
    
    try:
        result = await capture_sidebar_debug()
        
        print("\n" + "=" * 40)
        print("📋 WORKFLOW COMPLETE")
        print("=" * 40)
        
        if result['screenshot_path']:
            print(f"📸 Screenshot: {result['screenshot_path']}")
            print(f"📐 Size: {result['screenshot_size']}")
        
        analysis = result['analysis']
        print(f"🔍 Analysis Summary:")
        print(f"   • Sidebar exists: {analysis['sidebarExists']}")
        print(f"   • AgentSelector count: {analysis['agentSelectorCount']}")
        print(f"   • Legacy div count: {analysis['agentDivCount']}")
        print(f"   • USERS & AGENTS text: {analysis['usersAgentsTextCount']}")
        
        if analysis['usersAgentsTextCount'] > 1:
            print("⚠️  POTENTIAL DUPLICATE ISSUE DETECTED")
        elif analysis['usersAgentsTextCount'] == 0:
            print("⚠️  NO USERS & AGENTS TEXT FOUND")
        else:
            print("✅ Single USERS & AGENTS section found")
            
    except Exception as e:
        print(f"❌ Workflow failed: {e}")
        print("🔧 Ensure Continuum server is running and browser is connected")

if __name__ == "__main__":
    asyncio.run(main())