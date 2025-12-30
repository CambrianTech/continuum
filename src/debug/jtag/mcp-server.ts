#!/usr/bin/env tsx
/**
 * JTAG MCP Server - Model Context Protocol Server for JTAG Commands
 *
 * Exposes ALL JTAG commands as MCP tools, dynamically generated from command schemas.
 * One-to-one mapping: each JTAG command = one MCP tool.
 *
 * Usage:
 *   npx tsx mcp-server.ts
 *
 * Or register in Claude Desktop config:
 *   {
 *     "mcpServers": {
 *       "jtag": {
 *         "command": "npx",
 *         "args": ["tsx", "/path/to/mcp-server.ts"]
 *       }
 *     }
 *   }
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  type Tool,
} from '@modelcontextprotocol/sdk/types.js';
import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from './system/core/client/shared/JTAGClient';
import { loadInstanceConfigForContext } from './system/shared/BrowserSafeConfig.js';
import * as fs from 'fs';
import * as path from 'path';

// Image extensions we can return inline
const IMAGE_EXTENSIONS = ['.png', '.jpg', '.jpeg', '.gif', '.webp'];

/**
 * Check if a filepath is an image and return base64-encoded content
 */
function tryReadImageAsBase64(filepath: string): { data: string; mimeType: string } | null {
  if (!filepath || typeof filepath !== 'string') return null;

  const ext = path.extname(filepath).toLowerCase();
  if (!IMAGE_EXTENSIONS.includes(ext)) return null;

  try {
    if (!fs.existsSync(filepath)) return null;

    const buffer = fs.readFileSync(filepath);
    const base64 = buffer.toString('base64');

    const mimeTypes: Record<string, string> = {
      '.png': 'image/png',
      '.jpg': 'image/jpeg',
      '.jpeg': 'image/jpeg',
      '.gif': 'image/gif',
      '.webp': 'image/webp',
    };

    return {
      data: base64,
      mimeType: mimeTypes[ext] || 'image/png',
    };
  } catch {
    return null;
  }
}

/**
 * Extract image filepath from result object
 * Looks for common patterns: filepath, path, imagePath, filename (if it's absolute)
 */
function extractImagePath(result: any): string | null {
  if (!result || typeof result !== 'object') return null;

  // Common field names for image paths
  const pathFields = ['filepath', 'path', 'imagePath', 'screenshotPath', 'file'];

  for (const field of pathFields) {
    if (result[field] && typeof result[field] === 'string') {
      const ext = path.extname(result[field]).toLowerCase();
      if (IMAGE_EXTENSIONS.includes(ext)) {
        return result[field];
      }
    }
  }

  // Check filename if it's an absolute path
  if (result.filename && typeof result.filename === 'string' && path.isAbsolute(result.filename)) {
    const ext = path.extname(result.filename).toLowerCase();
    if (IMAGE_EXTENSIONS.includes(ext)) {
      return result.filename;
    }
  }

  return null;
}
import { spawn } from 'child_process';

// Load config for WebSocket connection
const instanceConfig = loadInstanceConfigForContext();

// Load command schemas
const schemasPath = path.join(__dirname, 'generated-command-schemas.json');
const schemas = JSON.parse(fs.readFileSync(schemasPath, 'utf-8'));

// Convert JTAG command schema to MCP tool schema
function commandToTool(command: any): Tool {
  const properties: Record<string, any> = {};
  const required: string[] = [];

  for (const [paramName, paramDef] of Object.entries(command.params || {})) {
    const def = paramDef as any;

    // Map JTAG types to JSON Schema types
    let jsonType = 'string';
    if (def.type === 'number') jsonType = 'number';
    else if (def.type === 'boolean') jsonType = 'boolean';
    else if (def.type === 'array') jsonType = 'array';
    else if (def.type === 'object') jsonType = 'object';

    properties[paramName] = {
      type: jsonType,
      description: def.description || `${paramName} parameter`,
    };

    if (def.required) {
      required.push(paramName);
    }
  }

  // Sanitize command name for MCP (replace / with _)
  const toolName = command.name.replace(/\//g, '_');

  return {
    name: toolName,
    description: `[JTAG] ${command.description || command.name}`,
    inputSchema: {
      type: 'object',
      properties,
      required: required.length > 0 ? required : undefined,
    },
  };
}

// Create MCP server
const server = new Server(
  {
    name: 'jtag-mcp-server',
    version: '1.0.0',
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

// Build tool mapping: MCP tool name -> JTAG command name
const toolToCommand: Record<string, string> = {};
const tools: Tool[] = [];

// Add special system control tool
tools.push({
  name: 'jtag_system_start',
  description: '[JTAG] Start the JTAG system if not running. Takes ~90 seconds to fully start.',
  inputSchema: {
    type: 'object',
    properties: {
      wait: {
        type: 'boolean',
        description: 'Wait for startup to complete (default: false, returns immediately)',
      },
    },
  },
});

for (const command of schemas.commands) {
  const tool = commandToTool(command);
  tools.push(tool);
  toolToCommand[tool.name] = command.name;
}

// Track if we've started the system
let systemStarted = false;
let systemStarting = false;

async function startJTAGSystem(): Promise<string> {
  if (systemStarting) {
    return 'JTAG system is already starting up. Please wait ~90 seconds.';
  }

  // Check if already running by trying to connect
  try {
    await getClient();
    return 'JTAG system is already running.';
  } catch {
    // Not running, start it
  }

  systemStarting = true;

  return new Promise((resolve) => {
    const proc = spawn('npm', ['start'], {
      cwd: __dirname,
      detached: true,
      stdio: 'ignore',
    });

    proc.unref(); // Don't wait for it

    // Give it a moment to start
    setTimeout(() => {
      systemStarting = false;
      systemStarted = true;
      resolve('JTAG system starting in background. Wait ~90 seconds, then retry your command.');
    }, 2000);
  });
}

// Handle list_tools request
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools };
});

// Shared client for command execution
let sharedClient: any = null;

async function getClient() {
  if (sharedClient) return sharedClient;

  const clientOptions: JTAGClientConnectOptions = {
    targetEnvironment: 'server',
    transportType: 'websocket',
    serverUrl: `ws://localhost:${instanceConfig.ports.websocket_server}`,
    enableFallback: false,
    context: {
      mcp: {
        server: 'jtag-mcp-server',
        timestamp: new Date().toISOString(),
      },
    },
  };

  try {
    const result = await JTAGClientServer.connect(clientOptions);
    sharedClient = result.client;
    return sharedClient;
  } catch (error) {
    sharedClient = null;

    const message = error instanceof Error ? error.message : String(error);
    const isConnectionRefused = message.includes('ECONNREFUSED') || message.includes('connect');

    if (isConnectionRefused) {
      throw new Error(
        `JTAG system not running. Use the jtag_system_start tool to start it, ` +
        `or manually run: cd ${__dirname} && npm start (wait ~90 seconds)`
      );
    }
    throw error;
  }
}

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  // Handle special system control tool
  if (name === 'jtag_system_start') {
    const message = await startJTAGSystem();
    return {
      content: [{ type: 'text', text: message }],
    };
  }

  // Map MCP tool name back to JTAG command name
  const commandName = toolToCommand[name];
  if (!commandName) {
    return {
      content: [
        {
          type: 'text',
          text: JSON.stringify({ error: `Unknown tool: ${name}` }),
        },
      ],
      isError: true,
    };
  }

  try {
    const client = await getClient();

    // Execute JTAG command
    const result = await (client as any).commands[commandName](args || {});

    // Clean up result (remove context/sessionId wrapper)
    let cleanResult = result;
    while (cleanResult && typeof cleanResult === 'object' && 'commandResult' in cleanResult) {
      cleanResult = cleanResult.commandResult;
    }
    if (cleanResult && typeof cleanResult === 'object') {
      const { context, sessionId, ...actualResult } = cleanResult;
      cleanResult = actualResult;
    }

    // Check if result contains an image - return it inline as base64
    const imagePath = extractImagePath(cleanResult);
    if (imagePath) {
      const imageData = tryReadImageAsBase64(imagePath);
      if (imageData) {
        return {
          content: [
            {
              type: 'image' as const,
              data: imageData.data,
              mimeType: imageData.mimeType,
            },
            {
              type: 'text',
              text: JSON.stringify(cleanResult, null, 2),
            },
          ],
        };
      }
    }

    return {
      content: [
        {
          type: 'text',
          text: JSON.stringify(cleanResult, null, 2),
        },
      ],
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return {
      content: [
        {
          type: 'text',
          text: JSON.stringify({ error: message, command: commandName }),
        },
      ],
      isError: true,
    };
  }
});

// Cleanup on exit
process.on('SIGINT', async () => {
  if (sharedClient) {
    await sharedClient.disconnect();
  }
  process.exit(0);
});

// Start server
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error('JTAG MCP Server started'); // stderr so it doesn't interfere with stdio transport
}

main().catch(console.error);
