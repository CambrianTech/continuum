#!/usr/bin/env python3
"""
Fix Project Registration
Ensure active projects show up when clients connect
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def fix_project_registration():
    print("🔧 FIXING PROJECT REGISTRATION")
    print("Ensuring active projects show up when clients connect")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        # Register with detailed project information
        project_info = {
            'agentId': 'continuum-client-active',
            'agentName': 'Continuum Client Active Project',
            'agentType': 'ai',
            'projectPath': '/Users/joel/Development/cambrian/continuum',
            'projectName': 'continuum',
            'clientType': 'python_client',
            'active': True,
            'timestamp': int(asyncio.get_event_loop().time() * 1000)
        }
        
        print(f"📋 Registering project: {project_info}")
        await client.register_agent(project_info)
        
        # Send project registration message
        project_message = {
            'type': 'project_register',
            'projectPath': '/Users/joel/Development/cambrian/continuum',
            'projectName': 'continuum',
            'status': 'active',
            'clientCount': 1,
            'agentCount': 1,
            'timestamp': int(asyncio.get_event_loop().time() * 1000)
        }
        
        print(f"📤 Sending project registration: {project_message}")
        
        # Try to send via WebSocket
        result = await client.js.execute(f"""
            console.log('📋 Sending project registration to server');
            if (window.ws && window.ws.readyState === 1) {{
                const message = {str(project_message).replace("'", '"')};
                window.ws.send(JSON.stringify(message));
                console.log('✅ Project registration sent');
                return 'PROJECT_REGISTERED';
            }} else {{
                return 'WEBSOCKET_UNAVAILABLE';
            }}
        """)
        
        print(f"Project registration result: {result}")
        
        # Also try to trigger a UI refresh
        ui_refresh = await client.js.execute("""
            console.log('🔄 Triggering UI refresh for active projects');
            
            // Try to find and refresh any project display elements
            const projectElements = document.querySelectorAll('[class*="project"], [id*="project"]');
            console.log('📋 Found project elements:', projectElements.length);
            
            projectElements.forEach((el, i) => {
                console.log('  ' + i + ':', el.className || el.id, el.textContent.slice(0, 50));
            });
            
            // Try to trigger any refresh events
            document.dispatchEvent(new CustomEvent('projectsUpdated'));
            
            return 'UI_REFRESH_ATTEMPTED';
        """)
        
        print(f"UI refresh result: {ui_refresh}")
        
        # Keep connection alive briefly to let registration take effect
        import time
        time.sleep(3)
        
        print("✅ Project registration and UI refresh completed")

asyncio.run(fix_project_registration())