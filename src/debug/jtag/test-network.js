#!/usr/bin/env node
/**
 * Real Network Tests - Actually test that the server works
 * 
 * These tests verify what the user is seeing:
 * 1. Can we connect to port 9002?
 * 2. Do API endpoints return data?
 * 3. Do static files load without 404?
 */

const http = require('http');

function testRequest(port, path, description) {
  return new Promise((resolve, reject) => {
    const req = http.get(`http://localhost:${port}${path}`, (res) => {
      let data = '';
      res.on('data', chunk => data += chunk);
      res.on('end', () => {
        console.log(`✅ ${description}`);
        console.log(`   Status: ${res.statusCode}`);
        console.log(`   Content-Length: ${data.length} bytes`);
        if (res.statusCode === 200) {
          resolve({ status: res.statusCode, data, headers: res.headers });
        } else {
          reject(new Error(`HTTP ${res.statusCode}: ${data.substring(0, 100)}`));
        }
      });
    });
    
    req.on('error', (error) => {
      console.log(`❌ ${description}`);
      console.log(`   Error: ${error.message}`);
      reject(error);
    });
    
    req.setTimeout(3000, () => {
      req.destroy();
      reject(new Error('Request timeout'));
    });
  });
}

async function runNetworkTests() {
  console.log('🧪 Starting Real Network Tests');
  console.log('================================');
  
  const tests = [
    { port: 9002, path: '/', desc: 'Demo homepage (HTML)' },
    { port: 9002, path: '/demo.css', desc: 'CSS file via router' },
    { port: 9002, path: '/jtag.js', desc: 'JTAG client script' },
    { port: 9002, path: '/demo.js', desc: 'Demo JavaScript' },
    { port: 9002, path: '/api/server-info', desc: 'API endpoint' },
    { port: 9001, path: '/', desc: 'JTAG WebSocket server (should fail on HTTP)' }
  ];
  
  let passed = 0;
  let failed = 0;
  
  for (const test of tests) {
    try {
      await testRequest(test.port, test.path, test.desc);
      passed++;
    } catch (error) {
      failed++;
    }
    console.log('');
  }
  
  console.log('📊 Test Results');
  console.log('===============');
  console.log(`✅ Passed: ${passed}`);
  console.log(`❌ Failed: ${failed}`);
  console.log(`📈 Success Rate: ${Math.round(passed / (passed + failed) * 100)}%`);
  
  if (failed > 0) {
    console.log('');
    console.log('🔍 The user was right - network requests are failing!');
    process.exit(1);
  } else {
    console.log('');
    console.log('🎉 All network tests passed - server is working!');
    process.exit(0);
  }
}

// Wait a moment for server to start, then run tests
setTimeout(runNetworkTests, 2000);