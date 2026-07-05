#!/usr/bin/env tsx
/**
 * FileSave ArtifactsDaemon Migration Test
 * Tests that FileSave now properly delegates to ArtifactsDaemon
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { promises as fs } from 'fs';

const execAsync = promisify(exec);

async function testFileSaveArtifacts() {
  console.log('💾 Testing FileSave → ArtifactsDaemon Migration');
  console.log('==============================================');
  
  try {
    // Test 1: Basic file save via CLI
    console.log('\n📝 Test 1: Basic file save operation');
    
    const testContent = `Test file content created at ${new Date().toISOString()}`;
    const filename = 'test-filesave-migration.txt';
    
    try {
      const { stdout, stderr } = await execAsync(`./jtag file/save --filepath="${filename}" --content="${testContent}"`);
      
      if (stdout.includes('SUCCESS') || stdout.includes('✅')) {
        console.log('✅ FileSave command executed successfully');
        console.log('Response:', stdout.trim().split('\n').slice(-3).join('\n')); // Last 3 lines
      } else {
        console.log('❌ FileSave command failed');
        console.log('STDOUT:', stdout);
        console.log('STDERR:', stderr);
      }
    } catch (error) {
      console.log('❌ FileSave CLI failed:', error.message);
    }
    
    // Test 2: Verify file was created via ArtifactsDaemon path
    console.log('\n📁 Test 2: Verify file created in correct location');
    
    const expectedPath = `examples/test-bench/.continuum/jtag/currentUser/${filename}`;
    
    try {
      const stats = await fs.stat(expectedPath);
      console.log(`✅ File exists: ${expectedPath} (${stats.size} bytes)`);
      
      const content = await fs.readFile(expectedPath, 'utf8');
      if (content.includes(testContent.substring(0, 20))) {
        console.log('✅ File content matches expected');
      } else {
        console.log('❌ File content mismatch');
        console.log('Expected:', testContent.substring(0, 50));
        console.log('Actual:', content.substring(0, 50));
      }
      
    } catch (error) {
      console.log(`❌ File not found at expected path: ${expectedPath}`);
      
      // Check alternative paths
      const altPaths = [
        `examples/test-bench/.continuum/jtag/system/${filename}`,
        `.continuum/jtag/currentUser/${filename}`,
        `.continuum/jtag/system/${filename}`
      ];
      
      for (const altPath of altPaths) {
        try {
          await fs.stat(altPath);
          console.log(`🔍 Found file at alternative path: ${altPath}`);
          break;
        } catch {
          // Not found at this path
        }
      }
    }
    
    console.log('\n🎉 FileSave → ArtifactsDaemon migration test complete!');
    
  } catch (error) {
    console.error('❌ Test failed:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  testFileSaveArtifacts();
}