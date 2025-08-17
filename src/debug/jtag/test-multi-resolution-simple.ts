#!/usr/bin/env tsx
/**
 * Simple Multi-Resolution Screenshot Test
 * Tests the completed multi-resolution functionality without complex server management
 */

import { promises as fs } from 'fs';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

async function testMultiResolution() {
  console.log('🧪 Testing Multi-Resolution Screenshot Functionality');
  console.log('====================================================');
  
  try {
    // Test 1: Multi-preset screenshot
    console.log('📸 Test 1: Multi-preset screenshot (mobile,desktop)');
    const { stdout, stderr } = await execAsync('./jtag screenshot --querySelector=body --presets=mobile,desktop --filename=test-multi');
    
    if (stdout.includes('SUCCESS')) {
      console.log('✅ Multi-preset command executed successfully');
    } else {
      console.log('❌ Multi-preset command failed');
      console.log('STDOUT:', stdout);
      console.log('STDERR:', stderr);
    }
    
    // Test 2: Verify files created
    console.log('📁 Test 2: Verifying files created');
    const screenshotDir = 'examples/test-bench/.continuum/jtag/currentUser/screenshots/';
    
    try {
      const files = await fs.readdir(screenshotDir);
      const multiFiles = files.filter(f => f.includes('test-multi'));
      
      console.log(`Found ${multiFiles.length} multi-resolution files:`, multiFiles);
      
      // Check specific files
      const expectedFiles = ['test-multi-mobile.png', 'test-multi-desktop.png'];
      for (const expectedFile of expectedFiles) {
        if (files.includes(expectedFile)) {
          const stats = await fs.stat(`${screenshotDir}${expectedFile}`);
          console.log(`✅ ${expectedFile}: ${stats.size} bytes`);
        } else {
          console.log(`❌ Missing: ${expectedFile}`);
        }
      }
      
    } catch (dirError) {
      console.log('❌ Cannot read screenshots directory:', dirError.message);
    }
    
    // Test 3: Single preset
    console.log('📸 Test 3: Single preset (thumbnail)');
    const { stdout: stdout2 } = await execAsync('./jtag screenshot --querySelector=body --presets=thumbnail --filename=test-thumb');
    
    if (stdout2.includes('SUCCESS')) {
      console.log('✅ Single preset command executed successfully');
    } else {
      console.log('❌ Single preset command failed');
    }
    
    console.log('🎉 Multi-resolution testing complete!');
    
  } catch (error) {
    console.error('❌ Test failed:', error.message);
    process.exit(1);
  }
}

if (require.main === module) {
  testMultiResolution();
}