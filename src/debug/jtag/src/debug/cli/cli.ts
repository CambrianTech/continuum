#!/usr/bin/env tsx
/**
 * JTAG CLI - Connect to running system and execute commands
 */

import { JTAGSystemServer } from '../../../server/JTAGSystemServer';

async function executeCommand() {
  const [command, ...args] = process.argv.slice(2);

  if (!command) {
    console.log('Usage: ./jtag <command> [...args]');
    console.log('Available commands: screenshot, navigate, click, type, get-text, wait-for-element');
    process.exit(0);
  }

  try {
    console.log(`🎯 Connecting to JTAG system for command: ${command}`);
    
    // Connect to the running system (should be started by shell script)
    const jtag = await JTAGSystemServer.connect();
    console.log(`🆔 Connected to session: ${jtag.getSessionId()}`);
    
    // Parse command arguments
    const params = parseArgs(args);
    
    // Execute the command
    console.log(`⚡ Executing: ${command}`);
    const result = await (jtag.commands as any)[command](params);
    
    console.log('✅ Command completed!');
    console.log('📋 Result:', result);
    
  } catch (error) {
    console.error('❌ Command failed:', error);
    console.log('💡 Make sure the JTAG system is running');
    process.exit(1);
  }
}

function parseArgs(args: string[]): Record<string, any> {
  const params: Record<string, any> = {};
  
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i]?.replace(/^--/, '');
    const value = args[i + 1];
    
    if (key && value) {
      params[key] = value;
    }
  }
  
  return params;
}

executeCommand();