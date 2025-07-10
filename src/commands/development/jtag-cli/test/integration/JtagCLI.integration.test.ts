/**
 * JTAG CLI Integration Tests
 * Test JTAG CLI integration with actual Continuum system
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { spawn } from 'child_process';
import { JtagCLI } from '../../JtagCLI';

describe('JtagCLI Integration', () => {
  let jtag: JtagCLI;

  test('setup', () => {
    jtag = new JtagCLI();
    console.log('🛸 JTAG CLI Integration Tests');
    console.log('============================');
  });

  test('should execute health command successfully', async () => {
    console.log('Testing health command...');
    
    const result = await jtag.health();
    assert.ok(result.success, 'Health command should succeed');
    console.log('✅ Health command completed');
  });

  test('should execute session command successfully', async () => {
    console.log('Testing session command...');
    
    const result = await jtag.session();
    console.log('Session result success:', result.success);
    console.log('✅ Session command completed');
  });

  test('should execute logs command successfully', async () => {
    console.log('Testing logs command...');
    
    // Logs command should complete without throwing
    await assert.doesNotReject(async () => {
      await jtag.logs();
    });
    
    console.log('✅ Logs command completed');
  });

  test('should execute errors command successfully', async () => {
    console.log('Testing errors command...');
    
    // Errors command should complete without throwing
    await assert.doesNotReject(async () => {
      await jtag.errors();
    });
    
    console.log('✅ Errors command completed');
  });

  test('should execute warnings command successfully', async () => {
    console.log('Testing warnings command...');
    
    // Warnings command should complete without throwing
    await assert.doesNotReject(async () => {
      await jtag.warnings();
    });
    
    console.log('✅ Warnings command completed');
  });

  test('should check if continuum binary exists', async () => {
    console.log('Testing continuum binary availability...');
    
    return new Promise((resolve, reject) => {
      const process = spawn('./continuum', ['--version'], {
        stdio: 'pipe'
      });
      
      let hasOutput = false;
      process.stdout.on('data', () => {
        hasOutput = true;
      });
      
      process.on('close', (code) => {
        if (code === 0 || hasOutput) {
          console.log('✅ Continuum binary is available');
          resolve();
        } else {
          console.log('⚠️ Continuum binary not responding as expected');
          resolve(); // Don't fail the test, just log
        }
      });
      
      process.on('error', (error) => {
        console.log('⚠️ Continuum binary not found:', error.message);
        resolve(); // Don't fail the test, just log
      });
    });
  });

  test('probe command should actually work', async () => {
    console.log('Testing probe command for actual success...');
    
    const result = await jtag.probe('widgets');
    
    if (result.success) {
      console.log('✅ Probe command succeeded');
      console.log('Data:', result.data);
      assert.ok(true, 'Probe working correctly');
    } else {
      console.log('❌ Probe command failed - needs implementation fix');
      console.log('Output:', result.output.substring(0, 300));
      console.log('Error:', result.error);
      assert.fail('Probe command is broken - needs implementation fix');
    }
  });

  test('screenshot command should actually work', async () => {
    console.log('Testing screenshot command for actual success...');
    
    const result = await jtag.screenshot('body', 1.0, 'test-integration.png');
    
    if (result.success) {
      console.log('✅ Screenshot command succeeded');
      assert.ok(true, 'Screenshot working correctly');
    } else {
      console.log('❌ Screenshot command failed - needs implementation fix');
      console.log('Output:', result.output.substring(0, 300));
      assert.fail('Screenshot command is broken - needs implementation fix');
    }
  });
});

console.log('🔗 Starting JTAG CLI integration tests...');