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
import sharp from 'sharp';

// =============================================================================
// TOOL CATEGORIES - Common tools listed first for discoverability
// =============================================================================

/**
 * Tool category definitions with priority (lower = higher priority, shown first)
 * Common/essential tools should be priority 0-1
 */
const TOOL_CATEGORIES: Record<string, { priority: number; description: string }> = {
  // Essential - always needed
  'ping': { priority: 0, description: 'System health check' },
  'help': { priority: 0, description: 'Documentation' },
  'list': { priority: 0, description: 'List commands' },

  // Common interface tools
  'interface/screenshot': { priority: 1, description: 'Screenshot capture' },
  'interface/navigate': { priority: 1, description: 'Browser navigation' },
  'interface/click': { priority: 1, description: 'UI interaction' },

  // Common collaboration tools
  'collaboration/chat/send': { priority: 1, description: 'Send chat message' },
  'collaboration/chat/export': { priority: 1, description: 'Export chat' },
  'collaboration/chat/poll': { priority: 1, description: 'Poll for messages' },

  // AI tools
  'ai/generate': { priority: 2, description: 'AI text generation' },
  'ai/status': { priority: 2, description: 'AI persona status' },
  'ai/thoughtstream': { priority: 2, description: 'AI thought inspection' },

  // Data tools
  'data/list': { priority: 3, description: 'Query data' },
  'data/create': { priority: 3, description: 'Create records' },

  // Category prefixes for bulk assignment
  'interface/': { priority: 10, description: 'Interface commands' },
  'collaboration/': { priority: 20, description: 'Collaboration commands' },
  'ai/': { priority: 30, description: 'AI commands' },
  'data/': { priority: 40, description: 'Data commands' },
  'workspace/': { priority: 50, description: 'Workspace commands' },
  'development/': { priority: 60, description: 'Development commands' },
  'media/': { priority: 70, description: 'Media commands' },
  'system/': { priority: 80, description: 'System commands' },
};

/**
 * Get priority for a command (lower = shown first)
 */
function getCommandPriority(commandName: string): number {
  // Check exact match first
  if (TOOL_CATEGORIES[commandName]) {
    return TOOL_CATEGORIES[commandName].priority;
  }

  // Check prefix matches
  for (const [prefix, config] of Object.entries(TOOL_CATEGORIES)) {
    if (prefix.endsWith('/') && commandName.startsWith(prefix)) {
      return config.priority;
    }
  }

  // Default priority for uncategorized
  return 100;
}

// =============================================================================
// IMAGE HANDLING - Resize for MCP transport
// =============================================================================

/** Maximum dimensions for MCP image transport (keeps base64 under ~200KB) */
const MCP_IMAGE_MAX_WIDTH = 1200;
const MCP_IMAGE_MAX_HEIGHT = 800;
const MCP_IMAGE_QUALITY = 70;

// Image extensions we can return inline
const IMAGE_EXTENSIONS = ['.png', '.jpg', '.jpeg', '.gif', '.webp'];

const MIME_TYPES: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
};

/**
 * Resize image and return base64-encoded content for MCP transport
 * Ensures images are under size limits for efficient transport
 */
async function resizeAndEncodeImage(filepath: string): Promise<{ data: string; mimeType: string } | null> {
  if (!filepath || typeof filepath !== 'string') return null;

  const ext = path.extname(filepath).toLowerCase();
  if (!IMAGE_EXTENSIONS.includes(ext)) return null;

  try {
    if (!fs.existsSync(filepath)) return null;

    // Use sharp to resize and compress
    const resizedBuffer = await sharp(filepath)
      .resize(MCP_IMAGE_MAX_WIDTH, MCP_IMAGE_MAX_HEIGHT, {
        fit: 'inside',  // Maintain aspect ratio, fit within bounds
        withoutEnlargement: true,  // Don't upscale small images
      })
      .jpeg({ quality: MCP_IMAGE_QUALITY })  // Convert to JPEG for consistent compression
      .toBuffer();

    const base64 = resizedBuffer.toString('base64');

    return {
      data: base64,
      mimeType: 'image/jpeg',  // Always JPEG after resize
    };
  } catch (error) {
    // Fallback to raw read if sharp fails
    try {
      const buffer = fs.readFileSync(filepath);
      return {
        data: buffer.toString('base64'),
        mimeType: MIME_TYPES[ext] || 'image/png',
      };
    } catch {
      return null;
    }
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
const commandPriorities: Map<string, number> = new Map();

// =============================================================================
// META-TOOLS - Tools for discovering and working with other tools
// =============================================================================

const systemStartTool: Tool = {
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
};

const searchToolsTool: Tool = {
  name: 'jtag_search_tools',
  description: '[JTAG] Search for tools by keyword. Returns matching tool names and descriptions. Use this to find relevant tools instead of scanning all 157.',
  inputSchema: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description: 'Search query - matches against tool names and descriptions (e.g., "screenshot", "css", "widget", "chat")',
      },
      category: {
        type: 'string',
        description: 'Optional category filter: interface, collaboration, ai, data, development, workspace, media, system',
      },
      limit: {
        type: 'number',
        description: 'Max results to return (default: 10)',
      },
    },
    required: ['query'],
  },
};

/**
 * Search tools by keyword
 */
function searchTools(query: string, category?: string, limit: number = 10): Array<{ name: string; description: string; category: string }> {
  const queryLower = query.toLowerCase();
  const results: Array<{ name: string; description: string; category: string; score: number }> = [];

  for (const command of schemas.commands) {
    const nameLower = command.name.toLowerCase();
    const descLower = (command.description || '').toLowerCase();

    // Category filter
    if (category) {
      const categoryPrefix = category.endsWith('/') ? category : `${category}/`;
      if (!nameLower.startsWith(categoryPrefix) && nameLower !== category) {
        continue;
      }
    }

    // Score matches
    let score = 0;
    if (nameLower.includes(queryLower)) score += 10;
    if (nameLower.startsWith(queryLower)) score += 5;
    if (descLower.includes(queryLower)) score += 3;

    // Exact segment match (e.g., "css" matches "widget-css" but scores higher than "discussion")
    const segments = nameLower.split(/[\/\-_]/);
    if (segments.includes(queryLower)) score += 8;

    if (score > 0) {
      // Determine category from name
      const cmdCategory = nameLower.includes('/') ? nameLower.split('/')[0] : 'root';
      results.push({
        name: command.name,
        description: command.description || command.name,
        category: cmdCategory,
        score,
      });
    }
  }

  // Sort by score descending, then name
  results.sort((a, b) => b.score - a.score || a.name.localeCompare(b.name));

  return results.slice(0, limit).map(({ name, description, category }) => ({
    name,
    description,
    category,
  }));
}

// Build tools with priority tracking - meta-tools first
const unsortedTools: Tool[] = [systemStartTool, searchToolsTool];
commandPriorities.set('jtag_system_start', -2);  // Always first
commandPriorities.set('jtag_search_tools', -1);  // Second (discovery tool)

for (const command of schemas.commands) {
  const tool = commandToTool(command);
  unsortedTools.push(tool);
  toolToCommand[tool.name] = command.name;
  commandPriorities.set(tool.name, getCommandPriority(command.name));
}

// Sort tools by priority (lower priority number = shown first)
const tools: Tool[] = unsortedTools.sort((a, b) => {
  const priorityA = commandPriorities.get(a.name) ?? 100;
  const priorityB = commandPriorities.get(b.name) ?? 100;
  if (priorityA !== priorityB) return priorityA - priorityB;
  // Secondary sort by name for consistency
  return a.name.localeCompare(b.name);
});

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

  // Handle meta-tools (no JTAG connection needed)
  if (name === 'jtag_system_start') {
    const message = await startJTAGSystem();
    return {
      content: [{ type: 'text', text: message }],
    };
  }

  if (name === 'jtag_search_tools') {
    const { query, category, limit } = (args || {}) as { query: string; category?: string; limit?: number };
    const results = searchTools(query, category, limit || 10);
    return {
      content: [{
        type: 'text',
        text: JSON.stringify({
          query,
          category: category || 'all',
          count: results.length,
          tools: results,
          hint: 'Use the tool name with mcp__jtag__ prefix, replacing / with _',
        }, null, 2),
      }],
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

    // Check if result contains an image - resize and return inline as base64
    const imagePath = extractImagePath(cleanResult);
    if (imagePath) {
      const imageData = await resizeAndEncodeImage(imagePath);
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
