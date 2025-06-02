#!/usr/bin/env node
/**
 * TEST CONTINUUM SPAWN
 * 
 * Tests if the Continuum spawn system actually works
 * Launches Claude instances via Continuum and tests responses
 */

// Simple test to verify claude --print works
const { exec } = require('child_process');

async function testClaude() {
  console.log('🧪 Testing Claude via Continuum approach...');
  
  try {
    const testPrompt = "You are a helpful assistant. What is 3 + 5?";
    const command = `claude --print "${testPrompt}"`;
    
    console.log(`📨 Testing command: ${command}`);
    
    exec(command, { timeout: 10000 }, (error, stdout, stderr) => {
      if (error) {
        console.log(`❌ Command failed: ${error.message}`);
        console.log(`stderr: ${stderr}`);
        return;
      }
      
      const response = stdout.trim();
      console.log(`✅ Claude responded: "${response}"`);
      
      if (response.includes('8') || response.toLowerCase().includes('eight')) {
        console.log('🎉 CONTINUUM APPROACH WORKS! Claude gave correct answer.');
      } else {
        console.log('⚠️  Claude responded but answer seems incorrect');
      }
    });
    
  } catch (error) {
    console.log(`❌ Test failed: ${error.message}`);
  }
}

// Test the approach
testClaude();

console.log('🔄 Test running... wait for results...');