#!/usr/bin/env tsx
/**
 * Integration Test: New User Experience Validation
 * 
 * Tests the CLI from a new user perspective to validate:
 * 1. Help system works and is user-friendly
 * 2. Examples match actual calls
 * 3. Parameters are clearly documented
 * 4. Commands can describe themselves
 */

import { strict as assert } from 'node:assert';
import { test, describe } from 'node:test';
import { spawn } from 'node:child_process';

describe('New User Experience Integration Tests', () => {
  
  /**
   * Test what a new user sees when they first try the CLI
   */
  test('new user journey: discovery → help → usage', async () => {
    console.log('\n🆕 NEW USER JOURNEY TEST\n');
    
    // Step 1: New user runs basic help
    console.log('1️⃣ New user runs: continuum help');
    const helpOutput = await runCLI(['help']);
    
    // Should be human-readable, not raw JSON
    assert.ok(!helpOutput.includes('"commands":['), 'Help should not be raw JSON');
    assert.ok(helpOutput.includes('continuum'), 'Help should mention continuum command');
    
    console.log('✅ General help output:');
    console.log(helpOutput);
    
    // Step 2: User asks for specific command help
    console.log('\n2️⃣ New user runs: continuum help screenshot');
    const screenshotHelpOutput = await runCLI(['help', 'screenshot']);
    
    console.log('📸 Screenshot help output:');
    console.log(screenshotHelpOutput);
    
    // Should show parameters and examples
    assert.ok(screenshotHelpOutput.includes('screenshot'), 'Should mention screenshot');
    
    // Step 3: User tries the examples from help
    console.log('\n3️⃣ New user runs: continuum screenshot --help');
    const directHelpOutput = await runCLI(['screenshot', '--help']);
    
    console.log('🔍 Direct command help output:');
    console.log(directHelpOutput);
    
    // Should provide usage information, not execute the command
    const isJustExecution = directHelpOutput.includes('"filename"') && 
                           directHelpOutput.includes('"timestamp"');
    
    if (isJustExecution) {
      console.log('❌ PROBLEM: Command executes instead of showing help');
    }
  });

  /**
   * Test parameter discovery - can the system tell users what parameters exist?
   */
  test('parameter discovery: can users find out what options are available?', async () => {
    console.log('\n🔍 PARAMETER DISCOVERY TEST\n');
    
    // Try various ways a user might discover parameters
    const discoveryAttempts = [
      ['screenshot', '--help'],
      ['help', 'screenshot'],
      ['screenshot', '--list-params'],
      ['screenshot', '--options'],
      ['screenshot', '?'],
    ];
    
    console.log('Testing parameter discovery methods:');
    
    for (const attempt of discoveryAttempts) {
      console.log(`\n📋 Trying: continuum ${attempt.join(' ')}`);
      const output = await runCLI(attempt);
      
      // Check if output contains parameter information
      const hasParamInfo = output.includes('selector') || 
                          output.includes('filename') || 
                          output.includes('parameter') ||
                          output.includes('option');
      
      console.log(hasParamInfo ? '✅ Contains parameter info' : '❌ No parameter info');
      console.log(`Output: ${output.substring(0, 200)}...`);
    }
  });

  /**
   * Test example validation - do the examples in help actually work?
   */
  test('example validation: do help examples match real command usage?', async () => {
    console.log('\n📝 EXAMPLE VALIDATION TEST\n');
    
    // Get the screenshot command definition to see what examples it claims to support
    console.log('1️⃣ Getting screenshot command definition...');
    
    // This should work if commands properly describe themselves
    try {
      const definitionOutput = await runCLI(['screenshot', '--definition']);
      console.log('Definition output:', definitionOutput);
    } catch (error) {
      console.log('❌ No --definition support');
    }
    
    // Test basic screenshot call
    console.log('\n2️⃣ Testing basic screenshot call...');
    const basicOutput = await runCLI(['screenshot']);
    console.log('Basic screenshot output:', basicOutput);
    
    // Test with parameters that should exist based on the README
    console.log('\n3️⃣ Testing screenshot with selector...');
    const selectorOutput = await runCLI(['screenshot', '--selector=body']);
    console.log('Selector screenshot output:', selectorOutput);
    
    // Test filename parameter
    console.log('\n4️⃣ Testing screenshot with filename...');
    const filenameOutput = await runCLI(['screenshot', '--filename=test.png']);
    console.log('Filename screenshot output:', filenameOutput);
  });

  /**
   * Test schema discoverability - can we automatically generate help from the command module?
   */
  test('schema discovery: can we auto-generate help from command definitions?', async () => {
    console.log('\n🔬 SCHEMA DISCOVERY TEST\n');
    
    // Commands should expose their schema/definition
    console.log('Testing if commands can describe their own schema...');
    
    // Method 1: Check if getDefinition() is accessible
    try {
      // This would require importing the command directly
      const { ScreenshotCommand } = await import('../../../browser/screenshot/ScreenshotCommand.js');
      const definition = ScreenshotCommand.getDefinition();
      
      console.log('✅ Screenshot command definition:');
      console.log(JSON.stringify(definition, null, 2));
      
      // Validate definition has required fields
      assert.ok(definition.name, 'Definition should have name');
      assert.ok(definition.description, 'Definition should have description');
      assert.ok(definition.parameters, 'Definition should have parameters');
      assert.ok(definition.examples, 'Definition should have examples');
      
      // Test if examples are actually valid
      if (definition.examples && definition.examples.length > 0) {
        console.log('\n📚 Testing examples from definition:');
        
        for (const example of definition.examples) {
          console.log(`\n📖 Example: ${example.description}`);
          console.log(`Command: ${example.command}`);
          
          // Try to parse the example as JSON to validate format
          try {
            const exampleParams = JSON.parse(example.command);
            console.log('✅ Example is valid JSON');
            console.log('Parameters:', Object.keys(exampleParams));
          } catch (error) {
            console.log('❌ Example is not valid JSON:', error.message);
          }
        }
      }
      
    } catch (error) {
      console.log('❌ Cannot import ScreenshotCommand:', error.message);
    }
  });

  /**
   * Test user-friendly output formatting
   */
  test('output formatting: is command output user-friendly?', async () => {
    console.log('\n🎨 OUTPUT FORMATTING TEST\n');
    
    // Test different command outputs
    const commands = [
      ['health'],
      ['agents'],
      ['screenshot'],
      ['help']
    ];
    
    for (const cmd of commands) {
      console.log(`\n🔍 Testing output format for: ${cmd.join(' ')}`);
      const output = await runCLI(cmd);
      
      // Check if output is user-friendly
      const isRawJSON = output.startsWith('{') && output.includes('"');
      const hasUserFriendlyElements = output.includes('✅') || 
                                     output.includes('📋') || 
                                     output.includes('Usage:') ||
                                     output.includes('Available commands:');
      
      console.log(isRawJSON ? '❌ Raw JSON output' : '✅ Formatted output');
      console.log(hasUserFriendlyElements ? '✅ User-friendly elements' : '❌ No user-friendly formatting');
      console.log(`Sample: ${output.substring(0, 100)}...`);
    }
  });

});

/**
 * Helper function to run CLI commands and capture output
 */
async function runCLI(args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn('npx', ['tsx', 'src/cli/continuum-cli.ts', ...args], {
      cwd: '/Volumes/FlashGordon/cambrian/continuum',
      stdio: 'pipe'
    });
    
    let output = '';
    let error = '';
    
    child.stdout?.on('data', (data) => {
      output += data.toString();
    });
    
    child.stderr?.on('data', (data) => {
      error += data.toString();
    });
    
    child.on('close', (code) => {
      if (code === 0) {
        resolve(output);
      } else {
        reject(new Error(`CLI failed with code ${code}: ${error}`));
      }
    });
    
    // Timeout after 10 seconds
    setTimeout(() => {
      child.kill();
      reject(new Error('CLI command timed out'));
    }, 10000);
  });
}

// Entry point for direct execution
if (import.meta.url === `file://${process.argv[1]}`) {
  console.log('🧪 Running New User Experience integration tests...');
}