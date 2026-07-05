#!/usr/bin/env tsx
/**
 * Transport Layer Diagnostic Test
 * 
 * Specifically diagnoses the bidirectional WebSocket communication failure
 * that's causing screenshot and cross-context commands to timeout.
 */

import { jtag } from '../server-index';
import { autoSpawnTest } from '../utils/TestAutoSpawn';

interface DiagnosticResult {
  test: string;
  success: boolean;
  duration: number;
  error?: string;
  details?: any;
}

async function diagnoseTransportIssue(): Promise<DiagnosticResult[]> {
  console.log('🔍 TRANSPORT LAYER DIAGNOSTIC');
  console.log('============================');
  
  const results: DiagnosticResult[] = [];
  
  try {
    console.log('🔗 Connecting to JTAG system...');
    const client = await jtag.connect({ targetEnvironment: 'server' });
    console.log('✅ Client connected successfully');
    
    // Test 1: Server-to-server command (should work)
    console.log('\n📋 Test 1: Server Command (ping) - Baseline');
    const pingStart = Date.now();
    try {
      const pingResult = await client.commands.ping();
      results.push({
        test: 'Server Ping',
        success: pingResult.success,
        duration: Date.now() - pingStart,
        details: { responseReceived: true, contextMatched: true }
      });
      console.log('✅ Server ping successful -', Date.now() - pingStart, 'ms');
    } catch (error) {
      results.push({
        test: 'Server Ping', 
        success: false,
        duration: Date.now() - pingStart,
        error: error.message
      });
      console.log('❌ Server ping failed:', error.message);
    }
    
    // Test 2: Browser command (screenshot) - currently broken
    console.log('\n📋 Test 2: Browser Command (screenshot) - The Problem');
    console.log('🔍 Expected: Server → Browser request, Browser → Server response');
    console.log('🔍 Actual: Server → Browser request works, Browser → Server response fails');
    
    const screenshotStart = Date.now();
    try {
      // Shorter timeout for faster diagnosis
      const timeoutPromise = new Promise((_, reject) => {
        setTimeout(() => reject(new Error('Diagnostic timeout after 10 seconds')), 10000);
      });
      
      const screenshotPromise = client.commands.screenshot('transport-diagnostic');
      
      const screenshotResult = await Promise.race([screenshotPromise, timeoutPromise]);
      
      results.push({
        test: 'Browser Screenshot',
        success: true,  // If we get here, it worked!
        duration: Date.now() - screenshotStart,
        details: { unexpectedSuccess: true, responseReceived: true }
      });
      console.log('✅ Screenshot successful (unexpected!) -', Date.now() - screenshotStart, 'ms');
      
    } catch (error) {
      const duration = Date.now() - screenshotStart;
      let diagnosticDetails: any = { responseReceived: false };
      
      if (error.message.includes('timeout')) {
        diagnosticDetails = {
          responseReceived: false,
          requestSent: true, // We know from logs that "type: request" is received
          browserReceived: true, // Browser logs show command execution
          browserResponseSent: false, // This is what we need to verify
          serverResponseReceived: false // This is the confirmed failure
        };
        console.log('❌ Screenshot failed with timeout -', duration, 'ms');
        console.log('🔍 DIAGNOSIS: Server → Browser request works, Browser → Server response broken');
      } else {
        diagnosticDetails.unexpectedError = error.message;
        console.log('❌ Screenshot failed with unexpected error:', error.message);
      }
      
      results.push({
        test: 'Browser Screenshot',
        success: false,
        duration,
        error: error.message,
        details: diagnosticDetails
      });
    }
    
    // Test 3: Another browser command to confirm pattern
    console.log('\n📋 Test 3: Another Browser Command (click) - Pattern Confirmation');
    const clickStart = Date.now();
    try {
      const timeoutPromise = new Promise((_, reject) => {
        setTimeout(() => reject(new Error('Diagnostic timeout after 5 seconds')), 5000);
      });
      
      const clickPromise = client.commands.click('body');
      const clickResult = await Promise.race([clickPromise, timeoutPromise]);
      
      results.push({
        test: 'Browser Click',
        success: true,
        duration: Date.now() - clickStart,
        details: { patternBroken: true }
      });
      console.log('✅ Click successful (pattern broken!) -', Date.now() - clickStart, 'ms');
      
    } catch (error) {
      results.push({
        test: 'Browser Click',
        success: false,
        duration: Date.now() - clickStart,
        error: error.message,
        details: { patternConfirmed: error.message.includes('timeout') }
      });
      
      if (error.message.includes('timeout')) {
        console.log('❌ Click also failed with timeout - pattern confirmed');
      } else {
        console.log('❌ Click failed with error:', error.message);
      }
    }
    
  } catch (error) {
    console.error('💥 Test setup failed:', error.message);
    results.push({
      test: 'Test Setup',
      success: false,
      duration: 0,
      error: error.message
    });
  }
  
  return results;
}

async function main() {
  const results = await diagnoseTransportIssue();
  
  console.log('\n🎯 DIAGNOSTIC RESULTS');
  console.log('====================');
  
  results.forEach((result, index) => {
    const status = result.success ? '✅ PASS' : '❌ FAIL';
    console.log(`${index + 1}. ${result.test}: ${status} (${result.duration}ms)`);
    
    if (result.error) {
      console.log(`   Error: ${result.error}`);
    }
    
    if (result.details) {
      console.log(`   Details:`, result.details);
    }
  });
  
  const passCount = results.filter(r => r.success).length;
  const totalCount = results.length;
  
  console.log(`\n📊 Summary: ${passCount}/${totalCount} tests passed`);
  
  if (passCount < totalCount) {
    console.log('\n🔧 TRANSPORT LAYER ISSUE CONFIRMED');
    console.log('Next steps: Fix bidirectional WebSocket communication in router/transport layer');
    throw new Error('Transport layer issue confirmed - bidirectional WebSocket communication broken');
  } else {
    console.log('\n✅ TRANSPORT LAYER HEALTHY');
    process.exit(0);
  }
}

// Run with auto-spawn capability
autoSpawnTest(main);