#!/usr/bin/env tsx
/**
 * JTAG CLI - Command Line Interface for JTAG System
 * 
 * Translates command line arguments to JTAGClient calls and handles responses.
 * This is the main entry point that ./jtag forwards to.
 */

import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from './system/core/client/shared/JTAGClient';
import { EntryPointAdapter } from './system/core/entry-points/EntryPointAdapter';
import { systemOrchestrator } from './system/orchestration/SystemOrchestrator';
import { loadInstanceConfigForContext } from './system/shared/BrowserSafeConfig.js';

// Load config once at startup
const instanceConfig = loadInstanceConfigForContext();

/**
 * AI-Friendly Help System - For Fresh AIs Learning JTAG
 */
function displayHelp() {
  console.log('🤖 JTAG - Global Debugging CLI for Any Node.js Project');
  console.log('=' .repeat(80));
  console.log('🎯 Install once globally, use anywhere in any project directory');
  console.log('📦 Installation: npm install -g @continuum/jtag');
  console.log('');
  
  console.log('📋 CORE COMMANDS (Works from any directory):');
  console.log('----------------------------------------');
  console.log('📸 SCREENSHOT:   jtag screenshot --querySelector="body" --filename="debug.png"');
  console.log('⚡ PING TEST:    jtag ping');
  console.log('📝 LIST ALL:     jtag list');
  console.log('🔧 EXECUTE JS:   jtag exec --code="return {test: \'success\'}" --environment="browser"');
  console.log(`🌐 NAVIGATE:     jtag navigate --url="http://localhost:${instanceConfig.ports.http_server}"`);
  console.log('🖱️ CLICK:        jtag click --selector="button.submit"');
  console.log('⌨️ TYPE:         jtag type --text="Hello world" --selector="input"');
  console.log('📄 FILE SAVE:    jtag file/save --path="output.txt" --content="Generated content"');
  console.log('📖 GET TEXT:     jtag get-text --selector="div.content"');
  console.log('⏳ WAIT:         jtag wait-for-element --selector="div.loaded"');
  console.log('');
  
  console.log('🚨 AI DEVELOPMENT WORKFLOW:');
  console.log('----------------------------------------');
  console.log('1. 📍 cd /your/project/directory');
  console.log('2. 📸 jtag screenshot --filename=debug-$(date +%s).png');
  console.log('3. 🔍 jtag ping  # Check system health');
  console.log('4. 📋 ls -la .continuum/jtag/currentUser/logs/  # Check logs');
  console.log('5. 🔄 jtag --restart  # Restart if needed');
  console.log('');
  
  console.log('💡 GLOBAL CLI PATTERNS:');
  console.log('----------------------------------------');
  console.log('• Works from ANY directory after global install');
  console.log('• Creates .continuum/jtag/ in current working directory');
  console.log('• Auto-starts system as needed (browser opens automatically)');
  console.log('• Screenshots saved to .continuum/jtag/currentUser/screenshots/');
  console.log('• Logs saved to .continuum/jtag/currentUser/logs/');
  console.log('• Use --filename with timestamps: debug-$(date +%s).png');
  console.log('');
  
  console.log('🔗 GETTING STARTED:');
  console.log('----------------------------------------');
  console.log('📦 npm install -g @continuum/jtag');
  console.log('📍 cd /your/project');
  console.log('📸 jtag screenshot  # System auto-starts, browser opens');
  console.log('🎉 Debug screenshots saved to .continuum/jtag/currentUser/screenshots/');
  console.log('');
  
  console.log('🚀 LIKE CLAUDE CODE: Install once globally, use everywhere!');
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
    const params: Record<string, string | boolean | number> = {};
    let i = 0;
    while (i < rawParams.length) {
      const arg = rawParams[i];
      if (arg?.startsWith('--')) {
        const argWithoutDashes = arg.replace(/^--/, '');
        
        // Handle --key=value format
        if (argWithoutDashes.includes('=')) {
          const [key, ...valueParts] = argWithoutDashes.split('=');
          const value = valueParts.join('='); // Handle values that contain =
          
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
          i++;
        } 
        // Handle --key value format
        else {
          const key = argWithoutDashes;
          const value = rawParams[i + 1];
          if (value !== undefined && !value.startsWith('--')) {
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
            i += 2;
          } else {
            // Boolean flag
            params[key] = true;
            i++;
          }
        }
      } else {
        i++;
      }
    }
    
    // INTELLIGENT ENTRY POINT: Adapts behavior based on detected agent type
    const entryPoint = new EntryPointAdapter({
      verbose: params.verbose,
      quiet: params.quiet,
      format: params.format || 'auto',
      showAgentInfo: !params.quiet
    });

    // Get agent context and behavior
    const agentContext = entryPoint.getAgentContext();
    const behavior = entryPoint.getBehavior();

    // Show debug params if verbose mode
    if (behavior.logLevel === 'verbose') {
      console.log(`🔧 DEBUG PARAMS:`, JSON.stringify(params, null, 2));
    }

    // Log agent detection based on intelligent behavior
    entryPoint.logAgentDetection();
    
    // Add environment routing parameter for exec commands
    if (command === 'exec' && params.environment) {
      // Keep environment param for routing within the system
      // Don't delete it - let the system route to the appropriate exec command
    }
    
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server', 
      transportType: 'websocket',
      serverUrl: `ws://localhost:${instanceConfig.ports.websocket_server}`,
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
    
    // Pure client - no server startup, just connect to existing server
    if (behavior.logLevel === 'verbose') {
      console.log('🔗 CLI connecting to existing server...');
    }
    
    // Conditionally suppress verbose connection logs based on agent behavior
    const originalLog = console.log;
    const originalWarn = console.warn;
    if (!entryPoint.shouldShowVerboseLogs()) {
      console.log = () => {}; // Suppress logs during connection for AI agents
      console.warn = () => {}; // Suppress warnings during connection for AI agents  
    }
    
    let client;
    try {
      const result = await JTAGClientServer.connect(clientOptions);
      client = result.client;
    } catch (err) {
      // Restore console logging first
      console.log = originalLog;
      console.warn = originalWarn;
      
      // Type guard for proper error handling
      const connectionError = err instanceof Error ? err : new Error(String(err));
      
      // Use consistent error formatting for all connection failures
      console.log('='.repeat(60));
      console.log('\x1b[31mERROR: Connection failed - ' + connectionError.message + '\x1b[0m');
      console.log('='.repeat(60));
      
      const isConnectionRefused = connectionError.message.includes('ECONNREFUSED') || 
                                connectionError.message.includes('connect') ||
                                (err && typeof err === 'object' && 'code' in err && err.code === 'ECONNREFUSED');
      
      if (isConnectionRefused) {
        console.error('🔍 PROBLEM: No JTAG system is currently running');
        console.error('✅ IMMEDIATE ACTION: Run "npm start" and wait 60 seconds');
      } else {
        console.error('🔍 Connection details:', connectionError.message);
        console.error('🔍 Error code:', connectionError.code || 'unknown');
      }
      
      process.exit(1);
    }
    
    // Restore console logging
    console.log = originalLog;
    console.warn = originalWarn;
    
    // Execute command with timeout for autonomous development
    try {
      const commandTimeout = new Promise((_, reject) => 
        setTimeout(() => reject(new Error(`Command '${command}' timed out after 10 seconds`)), 10000)
      );
      
      // Special parameter transformation for exec command
      if (command === 'exec') {
        // Transform CLI params to ExecCommandParams structure
        if (params.file) {
          // File-based execution
          params.code = {
            type: 'file',
            path: params.file
          };
          delete params.file;
        } else if (params.code && typeof params.code === 'string') {
          // Inline code execution
          params.code = {
            type: 'inline',
            language: params.language || 'javascript',
            source: params.code
          };
        }
        // Clean up CLI-specific params
        delete params.language;
      }
      
      // Special parameter transformation for screenshot command
      if (command === 'screenshot') {
        // Convert comma-separated presets to array
        if (params.presets && typeof params.presets === 'string') {
          params.options = params.options || {};
          params.options.presets = params.presets.split(',').map((p: string) => p.trim());
          delete params.presets;
        }
        
        // Convert comma-separated resolutions to array (if provided as JSON string)
        if (params.resolutions && typeof params.resolutions === 'string') {
          try {
            params.options = params.options || {};
            params.options.resolutions = JSON.parse(params.resolutions);
            delete params.resolutions;
          } catch (e) {
            console.warn(`⚠️ Invalid resolutions JSON: ${params.resolutions}`);
          }
        }
      }
      
      const commandExecution = (client as any).commands[command](params);
      
      const result = await Promise.race([commandExecution, commandTimeout]);
      
      // Show actual command result for debugging
      console.log('='.repeat(60));
      console.log('COMMAND RESULT:');
      console.log(JSON.stringify(result, null, 2));
      console.log('='.repeat(60));
      
      // CRITICAL: Properly disconnect client before exit to cleanup sessions
      await client.disconnect();
      
      process.exit(result?.success ? 0 : 1);
    } catch (err) {
      const cmdError = err instanceof Error ? err : new Error(String(err));
      console.log('='.repeat(60));
      console.log('\x1b[31mERROR: ' + cmdError.message + '\x1b[0m');
      console.log('='.repeat(60));
      if (cmdError.message.includes('timeout')) {
        console.error('🔍 Debug: Check system logs: npm run signal:errors');
      }
      
      // Cleanup client connection even on command failure
      if (client) {
        await client.disconnect();
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