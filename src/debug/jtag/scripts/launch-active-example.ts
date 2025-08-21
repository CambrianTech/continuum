#!/usr/bin/env npx tsx
/**
 * Launch Active Example Script
 * 
 * Starts BOTH the JTAG WebSocket server system AND the active example HTTP server
 * This ensures both services are running for proper JTAG functionality
 */

import { spawn, ChildProcess } from 'child_process';
import { JTAGSystemServer } from '../system/core/system/server/JTAGSystemServer';
import { getActiveExampleName, getActiveExamplePath } from '../system/shared/ExampleConfig';

let jtagServer: any = null;
let exampleServer: ChildProcess | null = null;
let keepAliveTimer: NodeJS.Timeout | null = null;

async function launchActiveExample() {
  try {
    console.log('🚀 Starting complete JTAG system with active example...');
    
    // 1. Start the JTAG WebSocket server system first
    console.log('🔌 Starting JTAG WebSocket Server System...');
    jtagServer = await JTAGSystemServer.connect();
    console.log('✅ JTAG WebSocket Server running on port 9001');
    
    // 2. Start the active example HTTP server
    const activeExampleName = getActiveExampleName();
    const activeExamplePath = getActiveExamplePath();
    
    console.log(`🌐 Starting ${activeExampleName} HTTP server...`);
    console.log(`📂 Example path: ${activeExamplePath}`);
    
    exampleServer = spawn('npm', ['start'], {
      cwd: activeExamplePath,
      stdio: 'inherit',
      shell: true
    });
    
    exampleServer.on('error', (error) => {
      console.error(`❌ Failed to launch ${activeExampleName}:`, error.message);
      cleanup();
      process.exit(1);
    });
    
    exampleServer.on('exit', (code) => {
      console.log(`📋 ${activeExampleName} exited with code ${code}`);
      cleanup();
      process.exit(code || 0);
    });
    
    console.log('✅ Complete JTAG system started successfully');
    console.log('🔌 JTAG WebSocket Server: ws://localhost:9001');
    console.log(`🌐 ${activeExampleName} HTTP Server: http://localhost:${activeExampleName === 'test-bench' ? '9002' : '9003'}`);
    
    // Setup cleanup handlers
    process.on('SIGINT', () => {
      console.log('\n⚡ Shutting down complete JTAG system...');
      cleanup();
      process.exit(0);
    });
    
    process.on('SIGTERM', () => {
      console.log('\n⚡ Terminating complete JTAG system...');
      cleanup();
      process.exit(0);
    });
    
    // Keep running - prevent Node.js from exiting
    console.log('📡 Complete JTAG system running - press Ctrl+C to stop both servers');
    
    // Keep the process alive with a simple timer
    keepAliveTimer = setInterval(() => {
      // Just keep the process alive - no actual work needed
    }, 30000); // Every 30 seconds
    
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    console.error('❌ Failed to launch complete JTAG system:', errorMsg);
    cleanup();
    process.exit(1);
  }
}

function cleanup() {
  if (keepAliveTimer) {
    clearInterval(keepAliveTimer);
    keepAliveTimer = null;
  }
  
  if (exampleServer) {
    console.log('🛑 Stopping example server...');
    exampleServer.kill('SIGTERM');
    exampleServer = null;
  }
  
  if (jtagServer) {
    console.log('🛑 Stopping JTAG server...');
    // TODO: Add proper server cleanup if available
    jtagServer = null;
  }
}

// Run the launcher
launchActiveExample();