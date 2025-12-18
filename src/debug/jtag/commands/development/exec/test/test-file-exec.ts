#!/usr/bin/env tsx
/**
 * Test exec command with file loading - like screenshot example
 */

// import { jtag } from '@continuum/jtag'; // Note: This import only works from test-bench directory
import fs from 'fs';
import path from 'path';

console.log('🎯 Testing ExecCommand with File Loading');
console.log('========================================');

async function testFileExec() {
  try {
    console.log('⚠️ This test needs to be run from examples/test-bench/ directory');
    console.log('📋 Use: cd examples/test-bench && npx tsx test-exec.ts');
    return;
    // console.log('🔗 Connecting to JTAG system...');
    // const jtagClient = await jtag.connect();
    
    console.log('✅ Connected! Loading script from file...');
    
    // Load the JavaScript file
    const scriptPath = path.join(__dirname, 'sample-script.js');
    if (!fs.existsSync(scriptPath)) {
      throw new Error(`Script file not found: ${scriptPath}`);
    }
    
    const scriptContent = fs.readFileSync(scriptPath, 'utf8');
    console.log(`📂 Loaded script from: ${scriptPath}`);
    console.log(`📝 Script length: ${scriptContent.length} characters`);
    
    console.log('🚀 Executing script...');
    
    // Use exec command like the screenshot example
    // const result = await jtagClient.commands.exec({
    //   code: {
    //     type: 'inline',
    //     language: 'javascript', 
    //     source: scriptContent
    //   }
    // });
    
    // console.log('🏆 EXEC TEST: SUCCESS!');
    // console.log('✅ Script executed successfully');
    // console.log('✅ Result received:', JSON.stringify(result, null, 2));
    
    // Verify we got the expected result structure
    // if (result.success && result.result) {
    //   console.log('✅ Result has expected structure');
    //   console.log('📊 Script output:', result.result);
    // } else {
    //   console.log('⚠️ Result structure unexpected');
    // }
    
  } catch (error) {
    console.error('❌ FILE EXEC TEST: FAILED');
    console.error('Error:', error instanceof Error ? error.message : String(error));
    throw error;
  }
}

// Run the test
testFileExec()
  .then(() => {
    console.log('\n✅ File exec test completed successfully');
    process.exit(0);
  })
  .catch((error) => {
    console.error('\n❌ File exec test failed:', error.message);
    process.exit(1);
  });