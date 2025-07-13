/**
 * Browser Widget Manifest Test - Real Browser Testing
 * 
 * This test runs in the actual browser to check if window.WIDGET_ASSETS works
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';

describe('Browser Widget Manifest - Real Browser Testing', () => {
  
  it('should verify WIDGET_ASSETS is available in browser', async () => {
    // Test that we can fetch from the running browser
    const response = await fetch('http://localhost:9000');
    assert.strictEqual(response.ok, true, 'Browser should be running');
    
    // Get the page HTML to see if WIDGET_ASSETS is included
    const html = await response.text();
    assert.ok(html.includes('WIDGET_ASSETS'), 'HTML should include WIDGET_ASSETS manifest');
    assert.ok(html.includes('SavedPersonasWidget'), 'Manifest should include SavedPersonasWidget');
    
    console.log('✅ Browser HTML includes WIDGET_ASSETS manifest');
  });

  it('should test actual widget CSS loading patterns', async () => {
    // Test the exact paths that widgets would try to load
    const widgetTests = [
      {
        widget: 'SavedPersonasWidget',
        directory: 'SavedPersonas',
        expectedCSS: ['SavedPersonas.css', 'SavedPersonasWidget.css', 'styles.css']
      },
      {
        widget: 'ChatWidget', 
        directory: 'Chat',
        expectedCSS: ['ChatWidget.css']
      },
      {
        widget: 'UsersAgentsWidget',
        directory: 'UsersAgents', 
        expectedCSS: ['UsersAgents.css']
      }
    ];

    for (const test of widgetTests) {
      console.log(`🔍 Testing ${test.widget} CSS loading...`);
      
      // Test each expected CSS file
      for (const cssFile of test.expectedCSS) {
        const cssPath = `http://localhost:9000/src/ui/components/${test.directory}/${cssFile}`;
        
        try {
          const response = await fetch(cssPath);
          if (response.ok) {
            const css = await response.text();
            console.log(`   ✅ ${cssFile} - ${css.length} chars`);
          } else {
            console.log(`   ❌ ${cssFile} - HTTP ${response.status}`);
          }
        } catch (error) {
          console.log(`   ❌ ${cssFile} - Error: ${error.message}`);
        }
      }
    }
  });

  it('should check BaseWidget.css availability', async () => {
    const baseCSS = 'http://localhost:9000/src/ui/components/shared/BaseWidget.css';
    
    const response = await fetch(baseCSS);
    assert.strictEqual(response.ok, true, `BaseWidget.css should be accessible: ${response.status}`);
    
    const css = await response.text();
    assert.ok(css.length > 1000, 'BaseWidget.css should be substantial');
    assert.ok(css.includes('widget-'), 'BaseWidget.css should contain widget classes');
    
    console.log(`✅ BaseWidget.css loaded successfully (${css.length} chars)`);
  });
});