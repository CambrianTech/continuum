#!/usr/bin/env node
/**
 * TEST SHELL CALL
 * 
 * Tests the exact shell calling mechanism the pool uses
 */

const { spawn } = require('child_process');

async function testShellCall() {
  console.log('🧪 Testing shell call mechanism...');
  
  return new Promise((resolve, reject) => {
    const prompt = 'You are a helpful assistant.\\n\\nUser: What is 2+2?\\n\\nAssistant:';
    const bashCommand = `claude --print "${prompt.replace(/"/g, '\\"')}"`;
    
    console.log(`📤 Command: ${bashCommand}`);
    
    const process = spawn('bash', ['-c', bashCommand], {
      stdio: ['pipe', 'pipe', 'pipe']
    });
    
    let stdout = '';
    let stderr = '';
    
    process.stdout.on('data', (data) => {
      stdout += data.toString();
      console.log(`📨 stdout chunk: "${data.toString()}"`);
    });
    
    process.stderr.on('data', (data) => {
      stderr += data.toString();
      console.log(`⚠️  stderr chunk: "${data.toString()}"`);
    });
    
    process.on('close', (code) => {
      console.log(`🔄 Process closed with code: ${code}`);
      console.log(`📋 Final stdout: "${stdout}"`);
      console.log(`📋 Final stderr: "${stderr}"`);
      
      if (stdout.trim()) {
        console.log(`✅ SUCCESS: Got response "${stdout.trim()}"`);
        resolve(stdout.trim());
      } else {
        console.log(`❌ FAILED: No stdout. Code: ${code}`);
        reject(new Error(`No response. Code: ${code}, stderr: ${stderr}`));
      }
    });
    
    process.on('error', (error) => {
      console.log(`❌ Process error: ${error.message}`);
      reject(new Error(`Process error: ${error.message}`));
    });
    
    // Timeout after 15 seconds
    setTimeout(() => {
      console.log('⏰ Timeout reached, killing process');
      process.kill();
      reject(new Error('Shell call timed out'));
    }, 15000);
  });
}

// Run the test
testShellCall().then(response => {
  console.log(`\\n🎉 Shell call mechanism works!`);
  console.log(`Response: "${response}"`);
}).catch(error => {
  console.error(`\\n💥 Shell call mechanism failed: ${error.message}`);
});