#!/usr/bin/env tsx
/**
 * JTAG CLI - Command Line Interface for JTAG System
 * 
 * Translates command line arguments to JTAGClient calls and handles responses.
 * This is the main entry point that ./jtag forwards to.
 */

import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from './system/core/client/shared/JTAGClient';
import { agentDetection } from './system/core/detection/AgentDetectionRegistry';

/**
 * AI-Friendly Help System - For Fresh AIs Learning JTAG
 */
function displayHelp() {
  console.log('🤖 JTAG COMMAND HELP - AI Autonomous Development System');
  console.log('=' .repeat(80));
  console.log('🎯 Mission: Universal debugging and automation for AI-driven development');
  console.log('');
  
  console.log('📋 CORE COMMANDS (Copy & Paste Ready):');
  console.log('----------------------------------------');
  console.log('📸 SCREENSHOT:   ./jtag screenshot --querySelector="body" --filename="debug.png"');
  console.log('⚡ PING TEST:    ./jtag ping');
  console.log('📝 LIST ALL:     ./jtag list');
  console.log('🔧 EXECUTE JS:   ./jtag exec --code="return {test: \'success\'}" --environment="browser"');
  console.log('🌐 NAVIGATE:     ./jtag navigate --url="http://localhost:9002"');
  console.log('🖱️ CLICK:        ./jtag click --selector="button.submit"');
  console.log('⌨️ TYPE:         ./jtag type --text="AI input" --selector="input"');
  console.log('📄 FILE SAVE:    ./jtag file/save --path="output.txt" --content="AI generated"');
  console.log('📖 GET TEXT:     ./jtag get-text --selector="div.content"');
  console.log('⏳ WAIT:         ./jtag wait-for-element --selector="div.loaded"');
  console.log('');
  
  console.log('🚨 AI DEVELOPMENT WORKFLOW (AUTONOMOUS):');
  console.log('----------------------------------------');
  console.log('1. 🔍 CHECK STATUS:       npm run agent:quick');
  console.log('2. 📸 VISUAL DEBUG:       ./jtag screenshot --filename=debug-$(date +%s).png');
  console.log('3. 🧪 RUN TESTS:          npm test');
  console.log('4. 📋 CHECK ERRORS:       tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log');
  console.log('5. 🔄 RESTART IF NEEDED:  npm run system:restart');
  console.log('');
  
  console.log('💡 AI COMMAND PATTERNS:');
  console.log('----------------------------------------');
  console.log('• All commands auto-start system if not running');
  console.log('• Auto-detects your identity (Claude, ChatGPT, Human, CI, etc.)');
  console.log('• Chat widgets get proper labels and persona info');
  console.log('• Use --filename with timestamps: debug-$(date +%s).png');
  console.log('• Chain commands: ./jtag ping && ./jtag screenshot');  
  console.log('• Check logs after any failures');
  console.log('• Use npm run agent for comprehensive status');
  console.log('');
  
  console.log('🔗 AI RESOURCES:');
  console.log('----------------------------------------');
  console.log('📚 Complete Guide:  cat dev-process.md');
  console.log('🎯 AI Dashboard:     npm run agent');  
  console.log('📊 Quick Status:     npm run agent:quick');
  console.log('🔧 Auto-Fix:         npm run agent:fix');
  console.log('📋 Error Logs:       ls -la examples/test-bench/.continuum/jtag/currentUser/logs/');
  console.log('');
  
  console.log('🚀 PERFECT FOR AI: No mysteries, clear errors, autonomous debugging!');
}

async function main() {
  try {
    // Parse command line arguments
    const args = process.argv.slice(2);
    const restartFlag = args.includes('--restart');
    const commandArgs = args.filter(arg => arg !== '--restart');
    
    if (commandArgs.length === 0) {
      console.log('Usage: ./jtag <command> [options]');
      console.log('Commands: screenshot, navigate, click, type, etc.');
      console.log('Try: ./jtag help');
      process.exit(1);
    }
    
    const [command, ...rawParams] = commandArgs;
    
    // Handle help command specially (for fresh AIs)
    if (command === 'help') {
      displayHelp();
      process.exit(0);
    }
    
    // Parse parameters into object format
    const params: any = {};
    for (let i = 0; i < rawParams.length; i += 2) {
      const key = rawParams[i]?.replace(/^--/, '');  // Remove -- prefix
      const value = rawParams[i + 1];
      if (key && value !== undefined) {
        // Try to parse JSON values, fall back to string
        let parsedValue = value;
        if (value.startsWith('{') || value.startsWith('[')) {
          try {
            parsedValue = JSON.parse(value);
          } catch (e) {
            // Keep as string if JSON parsing fails
            parsedValue = value;
          }
        }
        params[key] = parsedValue;
      }
    }
    
    // Debug: show what parameters we parsed
    console.log(`🔧 DEBUG PARAMS:`, JSON.stringify(params, null, 2));
    
    // AUTO-DETECTION: Automatically detects Claude, ChatGPT, Human, CI, etc.
    // Leaving parameters blank results in intelligent agent detection
    const agentContext = agentDetection.createConnectionContext();
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server', 
      transportType: 'websocket',
      serverUrl: 'ws://localhost:9001',
      enableFallback: false,
      context: {
        ...agentContext,
        cli: {
          command,
          args: commandArgs,
          timestamp: new Date().toISOString()
        }
      }
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
      
      // Show the actual result first for debugging
      console.log(`📋 FULL RESULT:`, JSON.stringify(result, null, 2));
      
      console.log(`✅ ${command}:`, result?.success ? 'SUCCESS' : 'FAILED');
      
      // Show any error messages
      if (result?.error) {
        console.log(`❌ Error: ${result.error}`);
      }
      if (result?.errorMessage) {
        console.log(`❌ Error Message: ${result.errorMessage}`);
      }
      
      // Add timing information for autonomous development 
      if (result?.timestamp) {
        const startTime = Date.now() - 10000; // Approximate start time
        const duration = new Date(result.timestamp).getTime() - startTime;
        console.log(`⏱️ Duration: ${Math.abs(duration)}ms`);
      }
      
      // Show key result data only
      if (result?.filepath) console.log(`📁 File: ${result.filepath}`);
      if (result?.commands) console.log(`📋 Found: ${result.commands.length} commands`);
      if (result?.commandResult?.filepath) console.log(`📁 File: ${result.commandResult.filepath}`);
      
      process.exit(result?.success ? 0 : 1);
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