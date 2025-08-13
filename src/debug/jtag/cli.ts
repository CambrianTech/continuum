#!/usr/bin/env tsx
/**
 * JTAG CLI - Command Line Interface for JTAG System
 * 
 * Translates command line arguments to JTAGClient calls and handles responses.
 * This is the main entry point that ./jtag forwards to.
 */

import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from './system/core/client/shared/JTAGClient';

async function main() {
  try {
    // Parse command line arguments
    const args = process.argv.slice(2);
    const restartFlag = args.includes('--restart');
    const commandArgs = args.filter(arg => arg !== '--restart');
    
    if (commandArgs.length === 0) {
      console.log('Usage: ./jtag <command> [options]');
      console.log('Commands: screenshot, navigate, click, type, etc.');
      process.exit(1);
    }
    
    const [command, ...rawParams] = commandArgs;
    
    // Parse parameters into object format
    const params: any = {};
    for (let i = 0; i < rawParams.length; i += 2) {
      const key = rawParams[i]?.replace(/^--/, '');  // Remove -- prefix
      const value = rawParams[i + 1];
      if (key && value !== undefined) {
        // Fix parameter key-value assignment
        params[key] = value;
      }
    }
    
    // Connect quietly to the running JTAG system
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server', 
      transportType: 'websocket',
      serverUrl: 'ws://localhost:9001',
      enableFallback: false 
    };
    
    // Suppress verbose connection logs by redirecting console temporarily
    const originalLog = console.log;
    const originalWarn = console.warn;
    console.log = () => {}; // Suppress logs during connection
    console.warn = () => {}; // Suppress warnings during connection
    
    const { client } = await JTAGClientServer.connect(clientOptions);
    
    // Restore console logging
    console.log = originalLog;
    console.warn = originalWarn;
    
    // Execute command with timeout for autonomous development
    try {
      const commandTimeout = new Promise((_, reject) => 
        setTimeout(() => reject(new Error(`Command '${command}' timed out after 10 seconds`)), 10000)
      );
      
      const commandExecution = (client as any).commands[command](params);
      
      const result = await Promise.race([commandExecution, commandTimeout]);
      
      console.log(`✅ ${command}:`, result.success ? 'SUCCESS' : 'FAILED');
      
      // Add timing information for autonomous development 
      if (result.timestamp) {
        const startTime = Date.now() - 10000; // Approximate start time
        const duration = new Date(result.timestamp).getTime() - startTime;
        console.log(`⏱️ Duration: ${Math.abs(duration)}ms`);
      }
      
      // Show key result data only
      if (result.filepath) console.log(`📁 File: ${result.filepath}`);
      if (result.commands) console.log(`📋 Found: ${result.commands.length} commands`);
      if (result.commandResult?.filepath) console.log(`📁 File: ${result.commandResult.filepath}`);
      
      process.exit(result.success ? 0 : 1);
    } catch (cmdError: any) {
      console.error(`❌ ${command} failed:`, cmdError.message);
      if (cmdError.message.includes('timeout')) {
        console.error('🔍 Debug: Check system logs: npm run signal:errors');
      }
      process.exit(1);
    }
    
  } catch (error) {
    console.error('❌ CLI Error:', error);
    process.exit(1);
  }
}

// Run the CLI
main().catch(console.error);