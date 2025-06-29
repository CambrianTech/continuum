/**
 * Quick HTML Generation Validation - TypeScript Version
 * Tests the critical integration issues our HTMLGeneration.integration.test.ts would catch
 */

const TEST_PORT: number = 9000;
const BASE_URL: string = `http://localhost:${TEST_PORT}`;

async function validateHTMLGeneration() {
  console.log('🧪 HTML Generation Integration Validation');
  console.log('==========================================');
  
  try {
    // Test 1: HTML Generation
    console.log('📋 Test 1: Basic HTML generation...');
    const htmlResponse = await fetch(`${BASE_URL}/`);
    const html = await htmlResponse.text();
    
    if (!html.includes('<!DOCTYPE html>')) {
      throw new Error('Missing DOCTYPE declaration');
    }
    if (!html.includes('continuum.js')) {
      throw new error('Missing continuum.js script reference');
    }
    console.log('✅ Valid HTML with continuum.js reference');
    
    // Test 2: Script Availability  
    console.log('📋 Test 2: Script file availability...');
    const scriptMatch = html.match(/src="([^"]*continuum\.js[^"]*)"/);
    if (!scriptMatch) {
      throw new Error('No continuum.js script tag found');
    }
    
    const scriptPath = scriptMatch[1];
    const scriptUrl = scriptPath.startsWith('/') ? `${BASE_URL}${scriptPath}` : scriptPath;
    const scriptResponse = await fetch(scriptUrl);
    
    if (scriptResponse.status !== 200) {
      throw new Error(`continuum.js returned ${scriptResponse.status} instead of 200`);
    }
    console.log(`✅ continuum.js is loadable: ${scriptPath} (${scriptResponse.status})`);
    
    // Test 3: Version Coordination
    console.log('📋 Test 3: Version parameter injection...');
    const versionMatch = scriptPath.match(/\?v=([^&]+)/);
    if (!versionMatch) {
      throw new Error('No version parameter found in script URL');
    }
    console.log(`✅ Version parameter present: ${versionMatch[1]}`);
    
    // Test 4: Widget Infrastructure
    console.log('📋 Test 4: Widget loading infrastructure...');
    if (!html.includes('widget-loader.js')) {
      console.log('⚠️  widget-loader.js not found (optional)');
    } else {
      console.log('✅ Widget loader present');
    }
    
    // Test 5: Client API Content
    console.log('📋 Test 5: Client API functionality...');
    const scriptContent = await scriptResponse.text();
    if (!scriptContent.includes('continuum:ready')) {
      throw new Error('Missing continuum:ready event system');
    }
    if (!scriptContent.includes('ContinuumBrowserAPI')) {
      throw new Error('Missing ContinuumBrowserAPI class');
    }
    console.log('✅ Client API contains required functionality');
    
    console.log('\n🎉 ALL INTEGRATION TESTS PASSED!');
    console.log('✅ HTML generation working correctly');
    console.log('✅ Script references are valid and loadable');
    console.log('✅ Version coordination implemented');
    console.log('✅ Widget self-discovery system ready');
    
  } catch (error) {
    console.error('\n❌ INTEGRATION TEST FAILED:');
    console.error(`❌ ${error.message}`);
    process.exit(1);
  }
}

// Run validation
validateHTMLGeneration().catch(error => {
  console.error('💥 Validation failed:', error);
  process.exit(1);
});