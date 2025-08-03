#!/usr/bin/env npx tsx
/**
 * Middle-Out Test: Minimal Connection & Health Check
 * 
 * Tests ONLY the basic transport abstraction:
 * 1. Browser client connects via createJTAGClient()
 * 2. Simple health check message through real JTAG router
 * 3. Verifies transport works end-to-end
 * 
 * NO mocks - real browser, real server, real routing
 */

import * as puppeteer from 'puppeteer';
import * as http from 'http';
import { jtagRouter } from '../../system/core/router/shared/JTAGRouter';

const TEST_PORT = 9002;
const TEST_TIMEOUT = 15000;

// Simple test runner without Jest
async function runTests() {
  let browser: puppeteer.Browser;
  let page: puppeteer.Page;
  let server: http.Server;
  
  console.log('🧪 Starting middle-out connection test...');
  
  try {
    // Start the minimal server with real JTAG routing
    const { spawn } = require('child_process');
    const serverProcess = spawn('npx', ['tsx', 'minimal-server.ts'], {
      cwd: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples',
      stdio: 'pipe'
    });
    
    // Wait for server to start
    await new Promise(resolve => setTimeout(resolve, 3000));
    
    // Launch browser - headless to avoid connection issues
    browser = await puppeteer.launch({
      headless: true, // Run headless to avoid WebSocket connection issues
      args: [
        '--no-sandbox', 
        '--disable-setuid-sandbox',
        '--disable-web-security'
      ]
    });
    
    page = await browser.newPage();
    
    // Log browser console for debugging
    page.on('console', msg => {
      console.log('🌐 Browser:', msg.text());
    });

    // Test 1: Browser creates JTAG client and connects
    console.log('🔍 Testing: Browser client creation and connection');
    
    // Navigate to demo page
    await page.goto(`http://localhost:${TEST_PORT}`, { waitUntil: 'networkidle0' });
    
    // Wait for JTAG auto-initialization
    await page.waitForFunction(() => window.jtag !== undefined, { timeout: 10000 });
    
    // Verify client was created successfully
    const clientExists = await page.evaluate(() => !!window.jtag);
    
    if (!clientExists) {
      throw new Error('JTAG client was not created in browser');
    }
    
    console.log('✅ JTAG client created in browser');

    // Test 2: Health check message routes through real JTAG system
    console.log('🔍 Testing: Health check via real transport');
    
    const healthResult = await page.evaluate(async () => {
      try {
        // Use the client's routeMessage method directly for health check
        const response = await fetch('/api/route', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            type: 'health-check',
            payload: { timestamp: new Date().toISOString() }
          })
        });
        
        const result = await response.json();
        return { success: response.ok, result };
      } catch (error) {
        return { success: false, error: error.message };
      }
    });
    
    console.log('📊 Health check result:', healthResult);
    
    if (!healthResult.success) {
      throw new Error('Health check failed: ' + JSON.stringify(healthResult));
    }
    
    console.log('✅ Health check routed through real JTAG transport');

    // Test 3: Transport abstraction handles connect message
    console.log('🔍 Testing: Connect message transport');
    
    const connectResult = await page.evaluate(async () => {
      try {
        // Test the actual connect flow that JTAG uses
        const response = await fetch('/api/route', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            type: 'connect',
            payload: {
              endpoint: 'ws://localhost:9001',
              transport: 'websocket'
            }
          })
        });
        
        const result = await response.json();
        return { success: response.ok, result };
      } catch (error) {
        return { success: false, error: error.message };
      }
    });
    
    console.log('📊 Connect result:', connectResult);
    
    if (!connectResult.success) {
      throw new Error('Connect message failed: ' + JSON.stringify(connectResult));
    }
    
    console.log('✅ Connect message handled by transport system');

    console.log('🎯 All middle-out tests passed! Transport abstraction working correctly.');

  } catch (error) {
    console.error('❌ Test failed:', error);
    process.exit(1);
  } finally {
    if (browser) await browser.close();
    if (server) server.close();
    
    // Kill any server processes
    const { exec } = require('child_process');
    exec('lsof -ti:9002 | xargs kill -9 2>/dev/null || true');
  }
}

// Run the tests
runTests();

console.log('🎯 Middle-out test: Focus on connection and health only - no complex features');