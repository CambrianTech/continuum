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
import * as fs from 'fs';
import * as path from 'path';

// Load config once at startup
const instanceConfig = loadInstanceConfigForContext();

/**
 * Get or create a persistent session ID for CLI continuity
 * This ensures all CLI commands in a session use the same browser session
 */
function getPersistentSessionId(): string | undefined {
  try {
    // Use the same path resolution as the instance config
    const exampleDir = instanceConfig.paths.directory;
    const sessionFile = path.join(exampleDir, '.continuum', 'jtag', 'cli-session-id.txt');
    
    // Check if we have a stored session ID
    if (fs.existsSync(sessionFile)) {
      const storedSessionId = fs.readFileSync(sessionFile, 'utf8').trim();
      
      // Validate session ID format (must be valid UUID)
      const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
      if (!uuidRegex.test(storedSessionId)) {
        console.log(`❌ Invalid session ID format in persistence file: ${storedSessionId}`);
        fs.unlinkSync(sessionFile);
        return undefined;
      }
      
      // Verify the session directory still exists (session is active)
      const sessionDir = path.join(exampleDir, '.continuum', 'jtag', 'sessions', 'user', storedSessionId);
      if (fs.existsSync(sessionDir)) {
        return storedSessionId;
      } else {
        // Session directory gone, remove stale session ID
        fs.unlinkSync(sessionFile);
      }
    }
    
    return undefined; // No valid existing session
  } catch (error) {
    // If anything fails, just let the system create a new session
    return undefined;
  }
}

/**
 * Store the session ID for future CLI commands
 */
function storePersistentSessionId(sessionId: string): void {
  try {
    // Use the same path resolution as the instance config
    const exampleDir = instanceConfig.paths.directory;
    const sessionFile = path.join(exampleDir, '.continuum', 'jtag', 'cli-session-id.txt');
    console.log(`🗂️ Storing session file at: ${sessionFile}`);
    fs.mkdirSync(path.dirname(sessionFile), { recursive: true });
    fs.writeFileSync(sessionFile, sessionId);
    console.log(`✅ Session ID stored successfully`);
  } catch (error) {
    console.log(`❌ Failed to store session ID: ${error}`);
  }
}

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
  console.log('🔗 SESSION PERSISTENCE:');
  console.log('----------------------------------------');
  console.log('✅ CLI automatically reuses browser sessions for continuity');
  console.log('🆕 jtag --new-session <command>  # Force new session');
  console.log('📝 Session state preserved across multiple CLI calls');
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
        // Handle positional arguments - add them to a general array
        if (!params._positional) {
          params._positional = [];
        }
        params._positional.push(arg);
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

    // Suppress ALL output unless verbose mode
    const originalStdoutWrite = process.stdout.write;
    const originalStderrWrite = process.stderr.write;

    if (behavior.logLevel !== 'verbose') {
      process.stdout.write = () => true;
      process.stderr.write = () => true;
    } else {
      console.log(`🔧 DEBUG PARAMS:`, JSON.stringify(params, null, 2));
      entryPoint.logAgentDetection();
    }
    
    // Add environment routing parameter for exec commands
    if (command === 'exec' && params.environment) {
      // Keep environment param for routing within the system
      // Don't delete it - let the system route to the appropriate exec command
    }
    
    // Session persistence: reuse existing session unless --new-session flag
    let sessionId: string | undefined;
    if (!params['new-session'] && command !== 'session/create') {
      sessionId = getPersistentSessionId();
      if (behavior.logLevel === 'verbose') {
        originalLog(`🔄 Session persistence: ${sessionId ? `reusing ${sessionId}` : 'creating new session'}`);
      }
    }
    
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server', 
      transportType: 'websocket',
      serverUrl: `ws://localhost:${instanceConfig.ports.websocket_server}`,
      enableFallback: false,
      sessionId: sessionId, // Use persistent session if available
      context: {
        ...agentContext,
        cli: {
          command,
          args: commandArgs,
          timestamp: new Date().toISOString(),
          sessionPersistence: sessionId ? 'reused' : 'new'
        }
      }
    };
    
    // Pure client - no server startup, just connect to existing server
    if (behavior.logLevel === 'verbose') {
      originalLog('🔗 CLI connecting to existing server...');
    }

    let client;
    try {
      const result = await JTAGClientServer.connect(clientOptions);
      client = result.client;
    } catch (err) {
      
      // Type guard for proper error handling
      const connectionError = err instanceof Error ? err : new Error(String(err));
      
      // Restore output streams first
      process.stdout.write = originalStdoutWrite;
      process.stderr.write = originalStderrWrite;

      const isConnectionRefused = connectionError.message.includes('ECONNREFUSED') ||
                                connectionError.message.includes('connect') ||
                                (err && typeof err === 'object' && 'code' in err && err.code === 'ECONNREFUSED');

      if (behavior.logLevel === 'verbose') {
        console.log('='.repeat(60));
        console.log('\x1b[31mERROR: Connection failed - ' + connectionError.message + '\x1b[0m');
        console.log('='.repeat(60));
        if (isConnectionRefused) {
          console.error('🔍 PROBLEM: No JTAG system is currently running');
          console.error('✅ IMMEDIATE ACTION: Run "npm start" and wait 60 seconds');
        } else {
          console.error('🔍 Connection details:', connectionError.message);
          console.error('🔍 Error code:', connectionError.code || 'unknown');
        }
      } else {
        // Clean JSON error for connection failures - send to stderr
        console.error(JSON.stringify({
          success: false,
          error: isConnectionRefused ?
            'No JTAG system running - run "npm start" and wait 60 seconds' :
            connectionError.message,
          timestamp: new Date().toISOString(),
          hint: "Use --verbose for detailed output"
        }, null, 2));
      }
      
      process.exit(1);
    }

    // Store session ID for persistence (will reuse browser session)
    if (behavior.logLevel === 'verbose') {
      console.log(`🔍 Session storage check: client.sessionId=${client.sessionId}, sessionId=${sessionId}`);
    }
    if (client.sessionId && !sessionId) {
      // Only store if we didn't already have one (new session created)
      if (behavior.logLevel === 'verbose') {
        console.log(`💾 Storing new session ID for persistence: ${client.sessionId}`);
      }
      storePersistentSessionId(client.sessionId);
    } else if (behavior.logLevel === 'verbose') {
      console.log(`⏭️ Skipping session storage - already had existing session or no client sessionId`);
    }
    
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

      // Show command result based on verbosity level
      if (behavior.logLevel === 'verbose') {
        console.log('='.repeat(60));
        console.log('COMMAND RESULT:');
        console.log(JSON.stringify(result, null, 2));
        console.log('='.repeat(60));
      } else {
        // Clean JSON output for non-verbose mode
        console.log(JSON.stringify(result, null, 2));
      }
      
      // Session persistence: only destroy session if explicitly requested
      if (params['new-session'] || command.startsWith('session/')) {
        // Explicit session management - disconnect fully
        await client.disconnect();
      } else {
        // Session persistence - disconnect transport only, keep session alive
        // The session remains active in the browser for next CLI call
        const transport = (client as any).getSystemTransport();
        if (transport) {
          await transport.disconnect();
          if (behavior.logLevel === 'verbose') {
            console.log('✅ JTAGClient: Transport disconnected (session preserved)');
          }
        }
      }

      // Restore output streams and send final result
      process.stdout.write = originalStdoutWrite;
      process.stderr.write = originalStderrWrite;

      // Output final result only
      if (behavior.logLevel === 'verbose') {
        console.log('='.repeat(60));
        console.log('COMMAND RESULT:');
        console.log(JSON.stringify(result, null, 2));
        console.log('='.repeat(60));
      } else {
        console.log(JSON.stringify(result, null, 2));
      }

      process.exit(result?.success ? 0 : 1);
    } catch (err) {
      const cmdError = err instanceof Error ? err : new Error(String(err));

      if (behavior.logLevel === 'verbose') {
        console.log('='.repeat(60));
        console.log('\x1b[31mERROR: ' + cmdError.message + '\x1b[0m');
        console.log('='.repeat(60));
        if (cmdError.message.includes('timeout')) {
          console.error('🔍 Debug: Check system logs: npm run signal:errors');
        }
      } else {
        // Clean JSON error output for non-verbose mode - send to stderr
        console.error(JSON.stringify({
          success: false,
          error: cmdError.message,
          timestamp: new Date().toISOString(),
          hint: "Use --verbose for detailed output"
        }, null, 2));
      }
      
      // Cleanup client connection even on command failure
      if (client) {
        await client.disconnect();
      }

      // Restore output streams and send error result
      process.stdout.write = originalStdoutWrite;
      process.stderr.write = originalStderrWrite;

      // Output error result to stderr
      console.error(JSON.stringify({
        success: false,
        error: cmdError.message,
        timestamp: new Date().toISOString(),
        hint: behavior.logLevel !== 'verbose' ? "Use --verbose for detailed output" : undefined
      }, null, 2));

      process.exit(1);
    }
    
  } catch (error) {
    console.error('❌ CLI Error:', error);
    process.exit(1);
  }
}

// Run the CLI
main().catch(console.error);