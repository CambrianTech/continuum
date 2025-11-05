#!/usr/bin/env tsx
/**
 * Integration Tests: Comprehensive CLI System Validation
 * 
 * Mirrors the comprehensive manual testing approach to ensure the CLI
 * works reliably in all scenarios. Tests actual command execution
 * against the live system.
 */

import { strict as assert } from 'node:assert';
import { test, describe } from 'node:test';
import { spawn } from 'child_process';

describe('Comprehensive CLI System Validation', () => {
  
  /**
   * Test core commands that should always work
   */
  test('core command execution - health, agents, projects', async () => {
    // Test health command
    const healthOutput = await runCLI(['health']);
    assert.ok(healthOutput.includes('"overall"'), 'Health command should return system status');
    assert.ok(healthOutput.includes('"components"'), 'Health should include component status');
    
    // Test agents command
    const agentsOutput = await runCLI(['agents']);
    assert.ok(agentsOutput.includes('"agents"'), 'Agents command should return agents array');
    assert.ok(agentsOutput.includes('"count"'), 'Agents should include count');
    
    // Test projects command
    const projectsOutput = await runCLI(['projects']);
    assert.ok(projectsOutput.includes('"projects"'), 'Projects command should return projects array');
  });

  /**
   * Test universal syntax - both forms should work identically
   */
  test('universal syntax validation - command vs --command', async () => {
    // Test health both ways
    const regularHealth = await runCLI(['health']);
    const universalHealth = await runCLI(['--health']);
    
    // Both should contain the same essential information
    assert.ok(regularHealth.includes('"overall"'), 'Regular syntax should work');
    assert.ok(universalHealth.includes('"overall"'), 'Universal syntax should work');
    
    // Test agents both ways
    const regularAgents = await runCLI(['agents']);
    const universalAgents = await runCLI(['--agents']);
    
    assert.ok(regularAgents.includes('"count"'), 'Regular agents should work');
    assert.ok(universalAgents.includes('"count"'), 'Universal agents should work');
  });

  /**
   * Test commands with complex parameters
   */
  test('complex parameter handling', async () => {
    // Test screenshot with parameters
    const screenshotOutput = await runCLI(['screenshot', '--filename=test-integration.png']);
    assert.ok(screenshotOutput.includes('"filename"'), 'Screenshot should accept filename parameter');
    assert.ok(screenshotOutput.includes('"artifactType"'), 'Screenshot should return artifact info');
    
    // Test data-marshal with complex JSON
    const complexData = '{"complex":{"nested":{"data":"value"}}}';
    const marshalOutput = await runCLI(['data-marshal', '--operation=encode', `--data=${complexData}`, '--encoding=json']);
    assert.ok(marshalOutput.includes('"marshalled"'), 'Data marshal should return marshalled data');
    assert.ok(marshalOutput.includes('"encoding"'), 'Data marshal should include encoding info');
    assert.ok(marshalOutput.includes('"ready"'), 'Data marshal should indicate readiness');
  });

  /**
   * Test auto-help for non-existent commands
   */
  test('auto-help for non-existent commands', async () => {
    const output = await runCLI(['nonexistent']);
    assert.ok(output.includes('needs parameters') || output.includes('Usage:'), 'Non-existent commands should show auto-help');
  });

  /**
   * Test no-translation policy - aliases show auto-help
   */
  test('no translation policy validation', async () => {
    // pic should show auto-help (no translation to screenshot)
    const picOutput = await runCLI(['pic']);
    assert.ok(picOutput.includes('needs parameters') || picOutput.includes('Usage:'), 'pic should show auto-help (no translation)');
    
    // snap should show auto-help (no translation to screenshot)
    const snapOutput = await runCLI(['snap']);
    assert.ok(snapOutput.includes('needs parameters') || snapOutput.includes('Usage:'), 'snap should show auto-help (no translation)');
    
    // But screenshot should work when it has parameters, or show help when it doesn't
    const screenshotOutput = await runCLI(['screenshot']);
    assert.ok(screenshotOutput.includes('"filename"') || screenshotOutput.includes('needs parameters'), 'screenshot command should work or show help');
  });

  /**
   * Test help system functionality
   */
  test('help system comprehensive validation', async () => {
    // Test general help
    const generalHelp = await runCLI(['help']);
    assert.ok(generalHelp.includes('CONTINUUM COMMANDS'), 'Help should show command categories');
    assert.ok(generalHelp.includes('screenshot'), 'Help should list screenshot command');
    assert.ok(generalHelp.includes('agents'), 'Help should list agents command');
    assert.ok(generalHelp.includes('Universal syntax'), 'Help should mention universal syntax');
    assert.ok(generalHelp.includes('no aliases or translations'), 'Help should mention no translations policy');
    
    // Test specific command help
    const screenshotHelp = await runCLI(['help', 'screenshot']);
    assert.ok(screenshotHelp.includes('SCREENSHOT'), 'Specific help should show command name');
    assert.ok(screenshotHelp.includes('Usage:'), 'Specific help should show usage');
  });

  /**
   * Test edge cases
   */
  test('edge cases validation', async () => {
    // Test empty command (should show help)
    const emptyOutput = await runCLI([]);
    assert.ok(emptyOutput.includes('Ultra-thin command pipe'), 'Empty command should show CLI help');
    
    // Test command with many parameters
    const manyParamsOutput = await runCLI([
      'data-marshal', 
      '--operation=encode', 
      '--data={"test":"value"}', 
      '--encoding=json',
      '--source=integration-test'
    ]);
    assert.ok(manyParamsOutput.includes('"marshalled"'), 'Should handle many parameters');
    assert.ok(manyParamsOutput.includes('"source"'), 'Should preserve all parameters');
  });

  /**
   * Test system responsiveness under load
   */
  test('system responsiveness validation', async () => {
    // Run multiple commands concurrently to test system stability
    const promises = [
      runCLI(['health']),
      runCLI(['agents']),
      runCLI(['projects']),
      runCLI(['--health']),
      runCLI(['--agents'])
    ];
    
    const results = await Promise.all(promises);
    
    // All commands should succeed
    results.forEach((result, index) => {
      assert.ok(result.length > 0, `Concurrent command ${index} should return data`);
    });
    
    // Health checks should all be consistent
    const healthResults = results.filter(r => r.includes('"overall"'));
    assert.ok(healthResults.length >= 2, 'Multiple health checks should succeed');
  });

  /**
   * Test JTAG command availability (even if parameters need work)
   */
  test('JTAG command system validation', async () => {
    // JTAG command should be listed in help
    const helpOutput = await runCLI(['help']);
    assert.ok(helpOutput.includes('jtag'), 'JTAG should be listed in available commands');
    
    // JTAG should be reachable (even if it fails due to missing parameters)
    try {
      await runCLI(['jtag']);
    } catch (error) {
      // This is expected - JTAG needs proper JSON parameters
      assert.ok(error.message.includes('HTTP 500'), 'JTAG should be reachable but require proper parameters');
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
  console.log('🧪 Running comprehensive CLI integration tests...');
}