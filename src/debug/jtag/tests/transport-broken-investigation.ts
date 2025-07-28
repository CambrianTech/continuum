#!/usr/bin/env tsx
/**
 * Transport Broken Investigation
 * 
 * Figure out exactly what's broken:
 * 1. Why are log files named test.undefined.json?
 * 2. Why is browser client not using transport abstraction?  
 * 3. What's actually happening with message routing?
 */

import { JTAGBase } from '@shared/JTAGBase';
import { jtagRouter } from '@shared/JTAGRouter';
import * as fs from 'fs';
import * as path from 'path';

const testLogDir = path.resolve(process.cwd(), '../../../.continuum/jtag/logs');

console.log('🔍 INVESTIGATING TRANSPORT SYSTEM FAILURES');
console.log('==========================================\n');

async function investigateLogging() {
  console.log('1. Testing log level handling...');
  
  // Initialize JTAG
  const config = {
    context: 'server' as const,
    jtagPort: 9001,
    enableRemoteLogging: true,
    enableConsoleOutput: true,
    maxBufferSize: 1000
  };

  try {
    JTAGBase.initialize(config);
    console.log('   ✅ JTAG initialized');
  } catch (error: any) {
    console.log('   ❌ JTAG initialization failed:', error.message);
    return;
  }

  // Test different log methods to see what creates undefined files
  console.log('   Testing log methods...');
  
  JTAGBase.log('INVESTIGATE', 'Regular log message');
  JTAGBase.error('INVESTIGATE', 'Error message');  
  JTAGBase.critical('INVESTIGATE', 'Critical message');
  JTAGBase.warn('INVESTIGATE', 'Warning message');

  // Wait for file operations
  await new Promise(resolve => setTimeout(resolve, 2000));

  // Check what files were created
  const logFiles = fs.readdirSync(testLogDir);
  console.log('   📁 Log files created:');
  logFiles.forEach(file => {
    console.log(`      - ${file}`);
  });

  const undefinedFiles = logFiles.filter(f => f.includes('undefined'));
  if (undefinedFiles.length > 0) {
    console.log('   ❌ BROKEN: Found undefined files:', undefinedFiles);
  } else {
    console.log('   ✅ No undefined files found');
  }
}

function investigateRouter() {
  console.log('\n2. Testing router configuration...');
  
  const transports = jtagRouter.getActiveTransports();
  console.log(`   📊 Active transports: ${transports.length}`);
  transports.forEach(transport => {
    console.log(`      - ${transport}`);
  });

  // Test message creation
  try {
    const testMessage = {
      id: 'investigate-001',
      type: 'log' as const,
      context: 'server' as const, 
      timestamp: Date.now(),
      payload: {
        level: 'info' as const,
        message: 'Investigation message',
        component: 'INVESTIGATE',
        data: { test: true }
      }
    };

    console.log('   📤 Testing message routing...');
    jtagRouter.routeMessage(testMessage);
    console.log('   ✅ Message routed successfully');
  } catch (error: any) {
    console.log('   ❌ Message routing failed:', error.message);
  }
}

function investigateBrowserClient() {
  console.log('\n3. Checking browser client implementation...');
  
  // Check what the compiled browser client actually does
  const browserClientPath = path.join(process.cwd(), 'examples/dist/jtag-auto-init.js');
  
  if (fs.existsSync(browserClientPath)) {
    const clientCode = fs.readFileSync(browserClientPath, 'utf8');
    
    if (clientCode.includes('new WebSocket(')) {
      console.log('   ❌ BROKEN: Browser client uses hardcoded WebSocket');
      const wsLines = clientCode.split('\n').filter(line => line.includes('WebSocket'));
      wsLines.forEach(line => {
        console.log(`      ${line.trim()}`);
      });
    } else {
      console.log('   ✅ Browser client not using hardcoded WebSocket');
    }

    if (clientCode.includes('jtagRouter') || clientCode.includes('transport')) {
      console.log('   ✅ Browser client uses transport abstraction');  
    } else {
      console.log('   ❌ BROKEN: Browser client does NOT use transport abstraction');
    }
  } else {
    console.log('   ❌ Browser client file not found');
  }
}

async function main() {
  await investigateLogging();
  investigateRouter();  
  investigateBrowserClient();
  
  console.log('\n🎯 SUMMARY');
  console.log('==========');
  console.log('The investigation should reveal exactly what\'s broken.');
  console.log('This is what proper debugging looks like - not fake passing tests.');
}

main().catch(console.error);