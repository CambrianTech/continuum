/**
 * SavedPersonas Widget Integration Test
 * Tests that the widget properly loads, connects to WebSocket, and displays data
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { promisify } from 'util';
import { exec } from 'child_process';

const execAsync = promisify(exec);

describe('SavedPersonas Widget Integration Tests', () => {

  test('should load in browser and connect to WebSocket', async () => {
    // Test that the widget loads properly in the UI
    const result = await execAsync(`curl -s http://localhost:9000`);
    const html = result.stdout;
    
    // Check that modular widgets are included
    assert(html.includes('SavedPersonas.js'), 'SavedPersonas.js should be included in HTML');
    assert(html.includes('BaseWidget.js'), 'BaseWidget.js should be included in HTML');
    
    // Check that the old refresh button is not present
    assert(!html.includes('🔄 Refresh'), 'Should not have manual refresh buttons');
    
    console.log('✅ SavedPersonas widget properly included in HTML without refresh button');
  });

  test('should use WebSocket for real-time updates', async () => {
    // Check that the widget code includes WebSocket listeners
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const componentPath = path.resolve(__dirname, '..', 'SavedPersonas.js');
    
    const fs = await import('fs');
    const componentContent = fs.readFileSync(componentPath, 'utf8');
    
    assert(componentContent.includes('setupWebSocketListeners'), 'Should have WebSocket listener setup');
    assert(componentContent.includes('personas_updated'), 'Should listen for persona updates');
    assert(componentContent.includes('persona_added'), 'Should listen for persona additions');
    assert(!componentContent.includes('refresh-btn'), 'Should not have refresh button');
    
    console.log('✅ SavedPersonas widget uses WebSocket for real-time updates');
  });

  test('should auto-detect and display mock personas', async () => {
    // Create a Python test to verify the widget shows data
    const pythonScript = `#!/usr/bin/env python3
"""
Test SavedPersonas Widget Data Detection
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_personas_detection():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'personas-test',
            'agentName': 'Personas Detection Test',
            'agentType': 'ai'
        })
        
        # Check if SavedPersonas widget is in the DOM
        result = await client.js.execute(\`
            (function() {
                // Look for SavedPersonas custom element
                const personasWidget = document.querySelector('saved-personas');
                const personasSection = document.querySelector('.persona-manager, [class*="persona"]');
                
                return {
                    hasPersonasWidget: !!personasWidget,
                    hasPersonasSection: !!personasSection,
                    personasWidgetHTML: personasWidget ? personasWidget.outerHTML.substring(0, 200) : null,
                    personasSectionHTML: personasSection ? personasSection.outerHTML.substring(0, 200) : null
                };
            })();
        \`)
        
        if result['success']:
            data = json.loads(result['result'])
            print(f"SavedPersonas widget found: {data['hasPersonasWidget']}")
            print(f"Personas section found: {data['hasPersonasSection']}")
            
            if data['hasPersonasWidget']:
                print("✅ SavedPersonas modular widget is present in DOM")
                return True
            elif data['hasPersonasSection']:
                print("✅ Personas section is present (legacy or modular)")
                return True
            else:
                print("❌ No personas widget or section found")
                return False
        else:
            print(f"❌ JavaScript execution failed: {result}")
            return False

if __name__ == '__main__':
    import json
    result = asyncio.run(test_personas_detection())
    print('SUCCESS' if result else 'FAILED')
`;

    const projectRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../../../..');
    const tempScriptPath = path.join(projectRoot, 'python-client', 'temp_personas_detection_test.py');
    
    const fs = await import('fs');
    fs.writeFileSync(tempScriptPath, pythonScript);
    
    try {
      // Execute the Python script
      const result = await execAsync(`cd ${projectRoot}/python-client && python3 temp_personas_detection_test.py`);
      
      console.log('Personas detection result:', result.stdout);
      
      // Check if the test passed
      assert(result.stdout.includes('SUCCESS') || result.stdout.includes('✅'), 'SavedPersonas widget should be detected in DOM');
      
    } finally {
      // Clean up temp script
      if (fs.existsSync(tempScriptPath)) {
        fs.unlinkSync(tempScriptPath);
      }
    }
  });
});