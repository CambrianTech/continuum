#!/usr/bin/env node
/**
 * MINIMAL CLAUDE WORKER
 * 
 * Absolutely minimal approach to get Claude working
 * Just one function that calls Claude and returns response
 */

const { exec } = require('child_process');

function callClaude(prompt) {
  return new Promise((resolve, reject) => {
    exec(`claude --print "${prompt.replace(/"/g, '\\"')}"`, (error, stdout, stderr) => {
      if (stdout && stdout.trim()) {
        resolve(stdout.trim());
      } else if (error && error.code !== 0) {
        reject(new Error(`Claude error: ${error.message}`));
      } else {
        reject(new Error('No response from Claude'));
      }
    });
  });
}

// Test it
async function test() {
  console.log('🧪 Testing minimal Claude...');
  
  try {
    const response = await callClaude('What is 2 + 2?');
    console.log(`✅ Claude: "${response}"`);
    
    // Test with role
    const response2 = await callClaude('You are a helpful assistant. User asks: I want to build a website. What questions should I ask them?');
    console.log(`✅ Claude (questioner): "${response2}"`);
    
    console.log('🎉 Minimal Claude is working!');
    
  } catch (error) {
    console.error(`❌ Failed: ${error.message}`);
  }
}

if (require.main === module) {
  test();
}

module.exports = { callClaude };