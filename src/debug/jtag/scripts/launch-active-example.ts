#!/usr/bin/env npx tsx
/**
 * Launch Active Example Script
 * 
 * Starts BOTH the JTAG WebSocket server system AND the active example HTTP server
 * This ensures both services are running for proper JTAG functionality
 */

import { spawn, ChildProcess } from 'child_process';
import { JTAGSystemServer } from '../system/core/system/server/JTAGSystemServer';
import { getActiveExampleName, getActiveExamplePath, getActivePorts } from '../system/shared/ExampleConfig';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';
import { ProcessCoordinator } from '../system/core/process/ProcessCoordinator';

let jtagServer: any = null;
let exampleServer: ChildProcess | null = null;
let keepAliveTimer: NodeJS.Timeout | null = null;

async function launchActiveExample() {
  const coordinator = ProcessCoordinator.getInstance();
  let lock: any = null;

  try {
    console.log('🚀 Intelligent JTAG system startup...');
    
    // Get active example configuration from examples.json ONLY
    let activePorts;
    let activeExamplePath;
    try {
      activePorts = getActivePorts();
      activeExamplePath = getActiveExamplePath();
      console.log(`🔧 Target ports: WebSocket=${activePorts.websocket_server}, HTTP=${activePorts.http_server}`);
      console.log(`📁 Active example path: ${activeExamplePath}`);
    } catch (error) {
      console.error('❌ CRITICAL FAILURE: Cannot load configuration from examples.json!');
      console.error('❌ Error:', error.message);
      throw new Error(`Configuration failure: ${error.message}`);
    }
    
    // Set context for per-project .continuum isolation
    const activeExample = getActiveExampleName();
    const workingDir = `examples/${activeExample}`;
    WorkingDirConfig.setWorkingDir(workingDir);
    console.log(`🎯 Context switched to: ${workingDir}`);

    // Acquire startup lock
    lock = await coordinator.acquireStartupLock(workingDir);
    
    // Plan intelligent startup
    const targetPorts = [activePorts.websocket_server, activePorts.http_server];
    const plan = await coordinator.planStartup(workingDir, targetPorts);
    
    console.log(`🧠 Startup plan: ${plan.type}`);
    
    if (plan.type === 'reuse_existing') {
      console.log(`✅ Reusing existing healthy server (PID: ${plan.process.pid})`);
      console.log(`🌐 Active ports: ${plan.process.ports.join(', ')}`);
      return; // Server already running and healthy
    }
    
    if (plan.type === 'graceful_handoff') {
      console.log(`🔄 Graceful handoff from context '${plan.from.context}' to '${workingDir}'`);
      // For now, kill old and start fresh (can improve later)
      try {
        process.kill(plan.from.pid, 'SIGTERM');
        await new Promise(resolve => setTimeout(resolve, 2000)); // Wait for graceful shutdown
      } catch (error) {
        console.warn(`⚠️ Failed to gracefully stop old process: ${error}`);
      }
    }
    
    // 1. Start the JTAG WebSocket server system first
    console.log('🔌 Starting JTAG WebSocket Server System...');
    jtagServer = await JTAGSystemServer.connect();
    console.log(`✅ JTAG WebSocket Server running on port ${activePorts.websocket_server}`);
    
    // Save process state for intelligent detection
    await coordinator.saveProcessState({
      pid: process.pid,
      ports: targetPorts,
      context: workingDir,
      startTime: new Date(),
      healthStatus: 'healthy'
    });
    
    // 2. Start the active example HTTP server  
    const activeExampleName = getActiveExampleName();
    
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
    console.log(`🔌 JTAG WebSocket Server: ws://localhost:${activePorts.websocket_server}`);
    console.log(`🌐 ${activeExampleName} HTTP Server: http://localhost:${activePorts.http_server}`);
    
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
  } finally {
    // Always release the startup lock
    if (lock) {
      await lock.release();
    }
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