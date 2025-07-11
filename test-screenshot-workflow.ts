/**
 * Quick Screenshot Workflow Test
 * 
 * Tests the complete screenshot + data marshal workflow without complex integration testing
 */

import { JtagCLI } from './src/commands/development/jtag-cli/JtagCLI';

async function testScreenshotWorkflow() {
  console.log('📸 Quick Screenshot Workflow Test');
  console.log('==================================');
  
  const jtag = new JtagCLI({
    continuumBinary: './continuum'
  });
  
  const testFilename = `test-screenshot-${Date.now()}.png`;
  console.log(`📁 Testing with filename: ${testFilename}`);
  
  try {
    // Test 1: Take screenshot
    console.log('📸 Step 1: Taking screenshot...');
    const screenshotResult = await jtag.screenshot('body', { 
      filename: testFilename,
      destination: 'file'
    });
    
    console.log('Screenshot result:', {
      success: screenshotResult.success,
      output: screenshotResult.output?.substring(0, 200) + '...',
      error: screenshotResult.error
    });
    
    // Test 2: Get session paths
    console.log('📁 Step 2: Getting session paths...');
    const pathsResult = await jtag.run('session-paths', { pathType: 'screenshots' });
    
    console.log('Session paths result:', {
      success: pathsResult.success,
      output: pathsResult.output?.substring(0, 200) + '...',
      error: pathsResult.error
    });
    
    // Test 3: Test data marshal
    console.log('🔗 Step 3: Testing data marshal...');
    const marshalResult = await jtag.run('data-marshal', {
      operation: 'encode',
      data: { filename: testFilename },
      encoding: 'json'
    });
    
    console.log('Data marshal result:', {
      success: marshalResult.success,
      output: marshalResult.output?.substring(0, 200) + '...',
      error: marshalResult.error
    });
    
    console.log('✅ All tests completed!');
    
  } catch (error) {
    console.error('❌ Test failed:', error);
  }
}

testScreenshotWorkflow();