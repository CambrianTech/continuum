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
    
    // Conditionally suppress verbose connection logs based on agent behavior
    const originalLog = console.log;
    const originalWarn = console.warn;
    if (!entryPoint.shouldShowVerboseLogs()) {
      console.log = () => {}; // Suppress logs during connection for AI agents
      console.warn = () => {}; // Suppress warnings during connection for AI agents  
    }
    
    const { client } = await JTAGClientServer.connect(clientOptions);
    
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
      
      const commandExecution = (client as any).commands[command](params);
      
      const result = await Promise.race([commandExecution, commandTimeout]);
      
      // Use intelligent output formatting based on agent type
      console.log(entryPoint.formatOutput(result));
      
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