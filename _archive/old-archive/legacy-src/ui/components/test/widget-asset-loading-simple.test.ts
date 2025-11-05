/**
 * Widget Asset Loading Tests - Detect Production Errors
 * 
 * These tests expose the exact asset loading errors from browser.error.json
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';

describe('Widget Asset Loading - Production Error Detection', () => {
  
  it('CRITICAL: BaseWidget.css must exist (currently failing in production)', async () => {
    // This test exposes: "🚨 MANIFEST ERROR: Failed to fetch /src/ui/components/shared/BaseWidget.css"
    
    try {
      const response = await fetch('http://localhost:9000/src/ui/components/shared/BaseWidget.css');
      
      if (!response.ok) {
        console.log(`❌ TEST CONFIRMED: BaseWidget.css missing (HTTP ${response.status})`);
        console.log(`   Expected by architecture docs but doesn't exist!`);
        assert.fail(`BaseWidget.css missing at /src/ui/components/shared/BaseWidget.css - HTTP ${response.status}`);
      }
      
      const css = await response.text();
      assert.ok(css.length > 0, 'BaseWidget.css should not be empty');
      console.log(`✅ BaseWidget.css found and non-empty (${css.length} characters)`);
      
    } catch (error) {
      console.log(`❌ TEST CONFIRMED: BaseWidget.css fetch error:`, error.message);
      assert.fail(`Failed to fetch BaseWidget.css: ${error.message}`);
    }
  });

  it('SavedPersonas widget CSS paths are inconsistent (multiple failed paths)', async () => {
    // These tests expose the errors from browser.error.json:
    // "Failed to fetch /src/ui/components/SavedPersonas/SavedPersonas.css"
    // "Failed to fetch /src/ui/components/SavedPersonas/SavedPersonasWidget.css" 
    // "Failed to fetch /src/ui/components/SavedPersonas/styles.css"
    
    const possiblePaths = [
      'http://localhost:9000/src/ui/components/SavedPersonas/SavedPersonas.css',
      'http://localhost:9000/src/ui/components/SavedPersonas/SavedPersonasWidget.css', 
      'http://localhost:9000/src/ui/components/SavedPersonas/styles.css'
    ];
    
    const results = [];
    
    for (const path of possiblePaths) {
      try {
        const response = await fetch(path);
        results.push({ path, status: response.status, ok: response.ok });
      } catch (error) {
        results.push({ path, status: 'ERROR', error: error.message });
      }
    }
    
    const successfulPaths = results.filter(r => r.ok);
    
    console.log(`🔍 SavedPersonas CSS path test results:`);
    results.forEach(r => {
      const status = r.ok ? '✅' : '❌';
      console.log(`   ${status} ${r.path} → ${r.status}`);
    });
    
    if (successfulPaths.length === 0) {
      console.log(`❌ TEST CONFIRMED: No valid SavedPersonas CSS files found!`);
      console.log(`   This proves the auto-derivation system is broken.`);
      assert.fail(`No SavedPersonas CSS files accessible. Results: ${JSON.stringify(results, null, 2)}`);
    }
    
    if (successfulPaths.length > 1) {
      console.log(`⚠️  Multiple valid paths found - inconsistent naming pattern!`);
      console.log(`   Architecture should have ONE canonical path.`);
    }
  });

  it('Other critical widget assets missing (from error log)', async () => {
    // Test other failing assets from browser.error.json
    const criticalAssets = [
      'http://localhost:9000/src/ui/components/UsersAgents/UsersAgents.css',
      'http://localhost:9000/src/ui/components/SessionCosts/SessionCosts.css',
      'http://localhost:9000/src/ui/components/SidebarHeader/SidebarHeader.html'
    ];
    
    const failures = [];
    
    for (const assetUrl of criticalAssets) {
      try {
        const response = await fetch(assetUrl);
        if (!response.ok) {
          failures.push({ url: assetUrl, status: response.status });
        }
      } catch (error) {
        failures.push({ url: assetUrl, error: error.message });
      }
    }
    
    if (failures.length > 0) {
      console.log(`❌ TEST CONFIRMED: ${failures.length} critical assets missing:`);
      failures.forEach(f => {
        console.log(`   ${f.url} → ${f.status || f.error}`);
      });
      
      assert.fail(`${failures.length} critical widget assets missing: ${JSON.stringify(failures, null, 2)}`);
    }
    
    console.log(`✅ All critical widget assets accessible`);
  });

  it('HTTP server serves /src/ui/components/ directory', async () => {
    // Verify the HTTP server configuration
    try {
      const response = await fetch('http://localhost:9000/src/ui/components/');
      
      // Should either return directory listing (200) or proper 404, but not 500/network error
      if (response.status === 500) {
        console.log(`❌ TEST CONFIRMED: Server error serving /src/ui/components/`);
        assert.fail(`HTTP server returning 500 for /src/ui/components/ - configuration issue`);
      }
      
      if (response.status === 0 || !response.status) {
        console.log(`❌ TEST CONFIRMED: Network error accessing server`);
        assert.fail(`Cannot reach HTTP server at localhost:9000 - is system running?`);
      }
      
      console.log(`✅ HTTP server responding for /src/ui/components/ (${response.status})`);
      
    } catch (error) {
      console.log(`❌ TEST CONFIRMED: HTTP server connection failed:`, error.message);
      assert.fail(`Cannot connect to HTTP server: ${error.message}`);
    }
  });
});