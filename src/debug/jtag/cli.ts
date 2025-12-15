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
import { DATA_COMMANDS } from './commands/data/shared/DataCommandConstants';
import { FILE_COMMANDS } from './commands/file/shared/FileCommandConstants';
import { USER_COMMANDS } from './commands/shared/SystemCommandConstants';
import { CODE_COMMANDS } from './commands/development/code/shared/CodeCommandConstants';
import * as fs from 'fs';
import * as path from 'path';

// Check for verbose flag EARLY to control module initialization logging
if (process.argv.includes('--verbose')) {
  process.env.JTAG_VERBOSE = 'true';
}

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

    // Parse parameters into object format
    const params: Record<string, string | boolean | number> = {};
    let i = 0;
    while (i < rawParams.length) {
      const arg = rawParams[i];
      if (arg && arg.startsWith('--')) {
        const argWithoutDashes = arg.replace(/^--/, '');

        // Handle --key=value format
        if (argWithoutDashes.includes('=')) {
          const [key, ...valueParts] = argWithoutDashes.split('=');
          const value = valueParts.join('='); // Handle values that contain =

          // SERDE-STYLE JSON PARSING: Try to parse as JSON primitives
          let parsedValue: any = value;

          // Always try JSON.parse for known JSON types
          if (value === 'true' || value === 'false' || value === 'null' ||
              value.startsWith('{') || value.startsWith('[') ||
              /^-?\d+(\.\d+)?$/.test(value)) { // Numbers (int or float)
            try {
              parsedValue = JSON.parse(value);
            } catch (e) {
              // Keep as string if JSON parsing fails
              parsedValue = value;
            }
          }

          // 🔧 NATURAL-IDIOMS: Accumulate repeated flags into arrays
          if (params[key] !== undefined) {
            // Key already exists - convert to array if needed
            if (!Array.isArray(params[key])) {
              params[key] = [params[key]];
            }
            params[key].push(parsedValue);
          } else {
            // First occurrence - store as-is
            params[key] = parsedValue;
          }
          i++;
        }
        // Handle --key value format
        else {
          const key = argWithoutDashes;
          const value = rawParams[i + 1];
          if (value !== undefined && !value.startsWith('--')) {
            // SERDE-STYLE JSON PARSING: Try to parse as JSON primitives
            let parsedValue: any = value;

            // Always try JSON.parse for known JSON types
            if (value === 'true' || value === 'false' || value === 'null' ||
                value.startsWith('{') || value.startsWith('[') ||
                /^-?\d+(\.\d+)?$/.test(value)) { // Numbers (int or float)
              try {
                parsedValue = JSON.parse(value);
              } catch (e) {
                // Keep as string if JSON parsing fails
                parsedValue = value;
              }
            }

            // 🔧 NATURAL-IDIOMS: Accumulate repeated flags into arrays
            if (params[key] !== undefined) {
              // Key already exists - convert to array if needed
              if (!Array.isArray(params[key])) {
                params[key] = [params[key]];
              }
              params[key].push(parsedValue);
            } else {
              // First occurrence - store as-is
              params[key] = parsedValue;
            }
            i += 2;
          } else {
            // Boolean flag
            params[key] = true;
            i++;
          }
        }
      } else if (arg && arg.includes('=') && !arg.startsWith('-')) {
        // Handle key=value format WITHOUT -- prefix (e.g., commandName=screenshot)
        const [key, ...valueParts] = arg.split('=');
        const value = valueParts.join('='); // Handle values that contain =

        // SERDE-STYLE JSON PARSING: Try to parse as JSON primitives
        let parsedValue: any = value;

        // Always try JSON.parse for known JSON types
        if (value === 'true' || value === 'false' || value === 'null' ||
            value.startsWith('{') || value.startsWith('[') ||
            /^-?\d+(\.\d+)?$/.test(value)) { // Numbers (int or float)
          try {
            parsedValue = JSON.parse(value);
          } catch (e) {
            // Keep as string if JSON parsing fails
            parsedValue = value;
          }
        }
        params[key] = parsedValue;
        i++;
      } else {
        // Handle positional arguments - add them to a general array
        if (!params._positional) {
          params._positional = [];
        }
        params._positional.push(arg);
        i++;
      }
    }

    // Handle positional arguments for single-parameter commands
    // This allows `./jtag help screenshot` instead of `./jtag help commandName=screenshot`
    if (params._positional && params._positional.length > 0) {
      // Map of commands to their primary parameter name
      const singleParamCommands: Record<string, string> = {
        'help': 'commandName',
        [CODE_COMMANDS.READ]: 'path',
        [CODE_COMMANDS.FIND]: 'pattern',
        [FILE_COMMANDS.LOAD]: 'path',
        [FILE_COMMANDS.SAVE]: 'path',
        [DATA_COMMANDS.READ]: 'id',
        [DATA_COMMANDS.DELETE]: 'id',
        [USER_COMMANDS.CREATE]: 'uniqueId',
        // Add more single-param commands as needed
      };

      const primaryParam = singleParamCommands[command];
      if (primaryParam && !params[primaryParam]) {
        // Use first positional arg as the primary parameter
        params[primaryParam] = params._positional[0];
        // Remove from positional array
        params._positional = params._positional.slice(1);
        if (params._positional.length === 0) {
          delete params._positional;
        }
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

    // Set environment variable for modules to check verbose mode
    if (behavior.logLevel === 'verbose') {
      process.env.JTAG_VERBOSE = 'true';
    }

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
        console.log(`🔄 Session persistence: ${sessionId ? `reusing ${sessionId}` : 'creating new session'}`);
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
      console.log('🔗 CLI connecting to existing server...');
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
    
    // Execute command with command-specific timeout
    try {
      // AI commands need longer timeout due to queue + generation time
      // Genome commands can take longer for training operations
      const isAICommand = command.startsWith('ai/');
      const isGenomeCommand = command.startsWith('genome/');
      const timeoutMs = isGenomeCommand ? 300000 : isAICommand ? 60000 : 10000; // 5min for genome, 60s for AI, 10s for others
      const timeoutSeconds = timeoutMs / 1000;

      const commandTimeout = new Promise((_, reject) =>
        setTimeout(() => reject(new Error(`Command '${command}' timed out after ${timeoutSeconds} seconds`)), timeoutMs)
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

      // Extract just the essential result, removing wrapper layers
      let cleanOutput = result;

      // Navigate to the innermost commandResult (the actual result)
      while (cleanOutput && typeof cleanOutput === 'object' && 'commandResult' in cleanOutput) {
        cleanOutput = cleanOutput.commandResult;
      }

      // Remove context and other wrapper fields, keep only the actual result
      if (cleanOutput && typeof cleanOutput === 'object') {
        const { context, sessionId, ...actualResult } = cleanOutput;
        // If no meaningful data remains after stripping context/sessionId, keep success field
        const hasData = Object.keys(actualResult).filter(key => key !== 'success').length > 0;
        if (!hasData && 'success' in cleanOutput) {
          cleanOutput = { success: cleanOutput.success };
        } else {
          cleanOutput = actualResult;
        }
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

      // Output final result only - choose between verbose and clean
      if (behavior.logLevel === 'verbose') {
        console.log('='.repeat(60));
        console.log('COMMAND RESULT:');
        console.log(JSON.stringify(result, null, 2));
        console.log('='.repeat(60));
      } else {
        console.log(JSON.stringify(cleanOutput, null, 2));
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
        // Clean JSON error output for non-verbose mode
        console.error(JSON.stringify({
          error: cmdError.message,
          hint: "For more detailed output, use --verbose flag"
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