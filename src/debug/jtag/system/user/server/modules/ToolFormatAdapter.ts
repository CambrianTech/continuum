/**
 * ToolFormatAdapter - Abstract base class for tool call format parsing
 *
 * Enables extensible support for multiple XML/HTML tool calling formats.
 * Each adapter implements its own regex matcher and parse logic.
 *
 * Usage:
 * 1. Extend this class
 * 2. Implement matches() with your regex
 * 3. Implement parse() with your extraction logic
 * 4. Use protected helpers for common XML parsing tasks
 */

export interface ToolCall {
  toolName: string;
  parameters: Record<string, string>;
}

export interface ToolCallMatch {
  fullMatch: string;
  startIndex: number;
  endIndex: number;
}

/**
 * Tool definition from ToolRegistry
 */
export interface ToolDefinition {
  name: string;
  description: string;
  parameters: {
    type: 'object';
    properties: Record<string, { type: string; description: string; required?: boolean }>;
    required: string[];
  };
  category?: string;
}

/**
 * Abstract adapter for bidirectional tool format handling
 * Handles BOTH directions:
 * 1. RAG → AI: Format tools for system prompt
 * 2. AI → System: Parse tool calls from response
 */
export abstract class ToolFormatAdapter {
  /**
   * Unique identifier for this adapter format
   */
  abstract readonly formatName: string;

  /**
   * Format tools for system prompt (RAG → AI direction)
   * Each adapter formats tools in the way their target model expects
   *
   * @param tools - Available tool definitions
   * @returns Formatted string for system prompt
   */
  abstract formatToolsForPrompt(tools: ToolDefinition[]): string;

  /**
   * Format tool results for context injection (System → AI direction)
   * Each adapter formats results in the way their target model expects
   *
   * @param results - Executed tool results
   * @returns Formatted string for context
   */
  abstract formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string;

  /**
   * Match all tool calls in text using this adapter's regex (AI → System direction)
   *
   * @param text - Response text containing tool calls
   * @returns Array of matches with position info
   */
  abstract matches(text: string): ToolCallMatch[];

  /**
   * Parse a matched tool call into structured ToolCall (AI → System direction)
   *
   * @param match - Matched tool call text
   * @returns Parsed ToolCall with name and parameters
   */
  abstract parse(match: ToolCallMatch): ToolCall | null;

  /**
   * Protected helper: Extract parameter tags from XML block
   * Finds all <paramName>value</paramName> pairs
   *
   * @param xmlBlock - XML text containing parameter tags
   * @returns Record of parameter name -> value
   */
  protected extractParameters(xmlBlock: string): Record<string, string> {
    const parameters: Record<string, string> = {};
    const paramRegex = /<(\w+)>(.*?)<\/\1>/gs;
    const paramMatches = xmlBlock.matchAll(paramRegex);

    for (const paramMatch of paramMatches) {
      const paramName = paramMatch[1];
      const paramValue = paramMatch[2].trim();
      parameters[paramName] = paramValue;
    }

    return parameters;
  }

  /**
   * Protected helper: Extract text content from specific XML tag
   *
   * @param xmlBlock - XML text
   * @param tagName - Tag name to extract
   * @returns Tag content or null if not found
   */
  protected extractTag(xmlBlock: string, tagName: string): string | null {
    const regex = new RegExp(`<${tagName}>(.*?)<\/${tagName}>`, 's');
    const match = xmlBlock.match(regex);
    return match ? match[1].trim() : null;
  }

  /**
   * Protected helper: Extract attribute value from opening tag
   *
   * @param xmlBlock - XML text
   * @param attributeName - Attribute name to extract
   * @returns Attribute value or null if not found
   */
  protected extractAttribute(xmlBlock: string, attributeName: string): string | null {
    const regex = new RegExp(`${attributeName}="([^"]+)"`, 'i');
    const match = xmlBlock.match(regex);
    return match ? match[1].trim() : null;
  }
}

/**
 * Adapter for old-style format: <tool name="...">...</tool>
 */
export class OldStyleToolAdapter extends ToolFormatAdapter {
  readonly formatName = 'old-style';

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    let output = 'Available Tools:\n\n';

    for (const tool of tools) {
      output += `Tool: ${tool.name}\n`;
      output += `Description: ${tool.description}\n`;
      output += `Parameters:\n`;

      for (const [paramName, paramDef] of Object.entries(tool.parameters.properties)) {
        const required = tool.parameters.required.includes(paramName) ? ' (required)' : ' (optional)';
        output += `  <${paramName}>${required}: ${paramDef.description}\n`;
      }

      output += `\nExample:\n<tool name="${tool.name}">\n`;
      for (const paramName of tool.parameters.required) {
        output += `  <${paramName}>value</${paramName}>\n`;
      }
      output += `</tool>\n\n---\n\n`;
    }

    output += '\nWhen you need to use tools, format them as shown above.\n';
    return output;
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    return results.map(r => {
      if (r.success && r.content) {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>success</status>\n<content>\n${r.content}\n</content>\n</tool_result>`;
      } else {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>error</status>\n<error>${r.error || 'Unknown error'}</error>\n</tool_result>`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];
    const regex = /<tool\s+name="[^"]+">.*?<\/tool>/gs;

    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({
        fullMatch: match[0],
        startIndex: match.index,
        endIndex: regex.lastIndex
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    const toolName = this.extractAttribute(match.fullMatch, 'name');
    if (!toolName) {
      const preview = match.fullMatch ? match.fullMatch.slice(0, 50) : '[empty match]';
      console.warn(`[OldStyleToolAdapter] Missing name attribute in: ${preview}...`);
      return null;
    }

    // Extract parameters - direct children tags
    const parameters = this.extractParameters(match.fullMatch);

    return { toolName, parameters };
  }
}

/**
 * Adapter for Anthropic-style format: <tool_use><tool_name>...</tool_name><parameters>...</parameters></tool_use>
 */
export class AnthropicStyleToolAdapter extends ToolFormatAdapter {
  readonly formatName = 'anthropic-style';

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    let output = 'Available Tools:\n\n';

    for (const tool of tools) {
      output += `Tool: ${tool.name}\n`;
      output += `Description: ${tool.description}\n`;
      if (tool.category) {
        output += `Category: ${tool.category}\n`;
      }
      output += `\nParameters:\n`;

      for (const [paramName, paramDef] of Object.entries(tool.parameters.properties)) {
        const required = tool.parameters.required.includes(paramName) ? ' (required)' : ' (optional)';
        output += `  - ${paramName}${required}: ${paramDef.description}\n`;
      }

      output += `\n---\n\n`;
    }

    output += `
=== HOW TO CALL TOOLS ===
Use this EXACT XML format to call tools:

<tool_use>
  <tool_name>TOOL_NAME_HERE</tool_name>
  <parameters>
    <param1>value1</param1>
    <param2>value2</param2>
  </parameters>
</tool_use>

Example - voting on a proposal:
<tool_use>
  <tool_name>collaboration/decision/vote</tool_name>
  <parameters>
    <proposalId>uuid-here</proposalId>
    <rankedChoices>["option1-id", "option2-id"]</rankedChoices>
  </parameters>
</tool_use>

CRITICAL RULES:
1. You MUST use the <tool_use> XML format above to call tools
2. NEVER write "Tool 'x' completed" - that is the RESULT format, not how you CALL tools
3. NEVER fabricate or make up tool results - the system will execute your tool call and return actual results
4. If you want to use a tool, output the <tool_use> XML block and WAIT for the result

When you need information, use tools instead of making assumptions.
`;

    return output;
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    return results.map(r => {
      if (r.success && r.content) {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>success</status>\n<content>\n${r.content}\n</content>\n</tool_result>`;
      } else {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>error</status>\n<error>\n\`\`\`\n${r.error || 'Unknown error'}\n\`\`\`\n</error>\n</tool_result>`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];
    const regex = /<tool_use>.*?<\/tool_use>/gs;

    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({
        fullMatch: match[0],
        startIndex: match.index,
        endIndex: regex.lastIndex
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    const toolName = this.extractTag(match.fullMatch, 'tool_name');
    if (!toolName) {
      const preview = match.fullMatch ? match.fullMatch.slice(0, 50) : '[empty match]';
      console.warn(`[AnthropicStyleToolAdapter] Missing <tool_name> in: ${preview}...`);
      return null;
    }

    // Extract parameters block, then parse parameters from it
    const parametersBlock = this.extractTag(match.fullMatch, 'parameters');
    const parameters = parametersBlock ? this.extractParameters(parametersBlock) : {};

    return { toolName, parameters };
  }
}

/**
 * Adapter for Markdown/Backtick format that local models (llama, etc.) often produce
 * Format: `tool: name` `param=value` `param2=value2`
 *
 * Examples:
 * - `tool: collaboration/dm` `participants=helper`
 * - `tool: read` `filepath=/path/to/file`
 */
export class MarkdownToolAdapter extends ToolFormatAdapter {
  readonly formatName = 'markdown-backtick';

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    // This adapter is for parsing, not prompting - use Anthropic format for prompts
    return '';
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    // Use XML format for results to clearly distinguish from tool calls
    // This prevents AIs from mimicking the result format when they should be calling tools
    return results.map(r => {
      if (r.success && r.content) {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>success</status>\n<content>\n${r.content}\n</content>\n</tool_result>`;
      } else {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>error</status>\n<error>\n${r.error || 'Unknown error'}\n</error>\n</tool_result>`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];

    // Match patterns like: `tool: name` followed by optional params `key=value`
    // Strategy: Find all `tool: X` occurrences and capture until end of line or next tool
    const lines = text.split('\n');
    let currentMatch = '';
    let startIndex = 0;
    let charOffset = 0;

    for (const line of lines) {
      const toolMatch = line.match(/`tool:\s*[^`]+`/i);

      if (toolMatch) {
        // Save previous match if exists
        if (currentMatch) {
          matches.push({
            fullMatch: currentMatch.trim(),
            startIndex,
            endIndex: charOffset
          });
        }
        // Start new match
        currentMatch = line;
        startIndex = charOffset + (toolMatch.index || 0);
      } else if (currentMatch && line.includes('`') && line.includes('=')) {
        // Continue current match with param line
        currentMatch += ' ' + line;
      }

      charOffset += line.length + 1; // +1 for newline
    }

    // Don't forget last match
    if (currentMatch) {
      matches.push({
        fullMatch: currentMatch.trim(),
        startIndex,
        endIndex: charOffset
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    // Extract tool name from first backtick section
    const toolNameMatch = match.fullMatch.match(/`tool:\s*([^`]+)`/i);
    if (!toolNameMatch) {
      return null;
    }

    const toolName = toolNameMatch[1].trim();
    const parameters: Record<string, string> = {};

    // Extract all param=value pairs from subsequent backtick sections
    const paramRegex = /`([^`=]+)=([^`]*)`/g;
    let paramMatch: RegExpExecArray | null;

    while ((paramMatch = paramRegex.exec(match.fullMatch)) !== null) {
      const paramName = paramMatch[1].trim();
      const paramValue = paramMatch[2].trim();
      if (paramName && paramName !== 'tool') {
        parameters[paramName] = paramValue;
      }
    }

    return { toolName, parameters };
  }
}

/**
 * Adapter for OpenAI/Generic function-call style format
 * Format: <function=tool_name>{"param": "value"}</function>
 *
 * This is what Groq, Together, and some other models naturally produce.
 * Examples from chat:
 * - <function=adapter_search> {"query": "embedding module"} </function>
 * - <function=code/search>{"query": "memory clustering"}</function>
 */
export class FunctionStyleToolAdapter extends ToolFormatAdapter {
  readonly formatName = 'function-style';

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    // Use Anthropic format for prompting, this is just for parsing
    return '';
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    return results.map(r => {
      if (r.success && r.content) {
        return `<function_result name="${r.toolName}" status="success">\n${r.content}\n</function_result>`;
      } else {
        return `<function_result name="${r.toolName}" status="error">\n${r.error || 'Unknown error'}\n</function_result>`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];

    // Match both proper XML and Groq's variant:
    //   <function=name>{json}</function>   — standard
    //   function=name>{json}               — Groq variant (no < prefix, no closing tag)
    // The regex uses optional < and optional </function> closing.
    const regex = /<?function=([^>\s]+)>\s*(\{[\s\S]*?\})(?:\s*<\/function>)?/gi;

    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({
        fullMatch: match[0],
        startIndex: match.index,
        endIndex: regex.lastIndex
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    // Extract tool name from <function=NAME> or function=NAME>
    const nameMatch = match.fullMatch.match(/<?function=([^>\s]+)>/i);
    if (!nameMatch) {
      return null;
    }

    const toolName = nameMatch[1].trim();
    const parameters: Record<string, string> = {};

    // Extract JSON body — find the first { ... } block after the >
    const jsonMatch = match.fullMatch.match(/>\s*(\{[\s\S]*\})/);
    if (jsonMatch && jsonMatch[1]) {
      const jsonStr = jsonMatch[1].trim();
      if (jsonStr) {
        try {
          const parsed = JSON.parse(jsonStr);
          // Flatten to string values for consistency with other adapters
          for (const [key, value] of Object.entries(parsed)) {
            parameters[key] = typeof value === 'string' ? value : JSON.stringify(value);
          }
        } catch {
          // If not valid JSON, try key=value parsing
          const kvMatch = jsonStr.match(/["']?(\w+)["']?\s*[:=]\s*["']?([^"',}]+)["']?/g);
          if (kvMatch) {
            for (const kv of kvMatch) {
              const [k, v] = kv.split(/[:=]/).map(s => s.trim().replace(/["']/g, ''));
              if (k && v) parameters[k] = v;
            }
          }
        }
      }
    }

    return { toolName, parameters };
  }
}

/**
 * Adapter for bare tool call format (no wrapping tags)
 * Format: tool_name {"param": "value"} or `tool_name` {json}
 *
 * This is what models often produce naturally:
 * - code/search {"query": "memory clustering", "path": "./src/"}
 * - code/tree {"path": "./workers/"}
 * - `code/tree` {"path": "."} (backtick-wrapped variant)
 */
export class BareToolCallAdapter extends ToolFormatAdapter {
  readonly formatName = 'bare-tool-call';

  // Known tool prefixes to identify tool calls
  private static TOOL_PREFIXES = [
    'code/', 'data/', 'collaboration/', 'ai/', 'voice/', 'search/',
    'workspace/', 'file/', 'interface/', 'genome/', 'adapter/',
    'persona/', 'runtime/', 'session/', 'user/', 'logs/', 'media/'
  ];

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    return ''; // Parsing only
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    return results.map(r => {
      if (r.success && r.content) {
        return `Tool ${r.toolName} succeeded:\n${r.content}`;
      } else {
        return `Tool ${r.toolName} failed: ${r.error || 'Unknown error'}`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];

    // Pattern: tool/name {json} or `tool/name` {json} (with optional backticks)
    // Must start with known prefix to avoid false positives
    // Supports both: code/tree {"path": "."} and `code/tree` {"path": "."}
    const prefixPattern = BareToolCallAdapter.TOOL_PREFIXES.map(p => p.replace('/', '\\/')).join('|');

    // Allow optional backticks around the tool name
    const regex = new RegExp(`\`?((?:${prefixPattern})[a-zA-Z0-9/_-]+)\`?\\s*(\\{[^{}]*(?:\\{[^{}]*\\}[^{}]*)*\\})`, 'g');

    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({
        fullMatch: match[0],
        startIndex: match.index,
        endIndex: regex.lastIndex
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    // Extract tool name and JSON (strip backticks if present)
    const prefixPattern = BareToolCallAdapter.TOOL_PREFIXES.map(p => p.replace('/', '\\/')).join('|');
    const parseRegex = new RegExp(`\`?((?:${prefixPattern})[a-zA-Z0-9/_-]+)\`?\\s*(\\{.+\\})`, 's');
    const parsed = match.fullMatch.match(parseRegex);

    if (!parsed) return null;

    const toolName = parsed[1].trim();
    const jsonStr = parsed[2].trim();
    const parameters: Record<string, string> = {};

    try {
      const parsedJson = JSON.parse(jsonStr);
      for (const [key, value] of Object.entries(parsedJson)) {
        parameters[key] = typeof value === 'string' ? value : JSON.stringify(value);
      }
    } catch {
      // Fallback: try to extract key-value pairs
      const kvRegex = /"([^"]+)":\s*"([^"]*)"/g;
      let kvMatch;
      while ((kvMatch = kvRegex.exec(jsonStr)) !== null) {
        parameters[kvMatch[1]] = kvMatch[2];
      }
    }

    return { toolName, parameters };
  }
}

/**
 * Registry of all supported tool format adapters
 * Add new adapters here to support additional formats
 *
 * Order matters for auto-detection - primary adapter is tried first
 */
export function getToolFormatAdapters(): ToolFormatAdapter[] {
  return [
    new AnthropicStyleToolAdapter(),  // Primary/default format
    new FunctionStyleToolAdapter(),   // OpenAI/Groq/Together function style
    new BareToolCallAdapter(),        // Bare tool_name {json} format
    new MarkdownToolAdapter(),         // Local model backtick format
    new OldStyleToolAdapter()          // Legacy XML support
  ];
}

/**
 * Get primary adapter for formatting (Anthropic style is our default)
 * Unless Anthropic's format is insufficient, we prefer using their standard
 * rather than inventing our own. All formats are supported via regex adapters.
 *
 * In future, this can be smart selection based on model config/recipe
 */
export function getPrimaryAdapter(): ToolFormatAdapter {
  return getToolFormatAdapters()[0];  // AnthropicStyleToolAdapter
}

// ========================
// Native Tool Support (JSON format for Anthropic API)
// ========================

import type { NativeToolSpec } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

/**
 * Sanitize tool name for Anthropic/OpenAI API
 * API requires: ^[a-zA-Z0-9_-]{1,128}$
 * Our tools have slashes: code/write → code_write
 */
export function sanitizeToolName(name: string): string {
  return ToolNameCodec.instance.encode(name);
}

/**
 * Restore original tool name from sanitized version (legacy — prefer ToolNameCodec)
 */
export function unsanitizeToolName(sanitizedName: string): string {
  return ToolNameCodec.instance.decode(sanitizedName);
}

/**
 * Bidirectional encoder/decoder for tool names sent over APIs.
 *
 * API constraint: Anthropic/OpenAI require tool names matching [a-zA-Z0-9_-]{1,64}.
 * Our tools use slashes: code/write, collaboration/chat/send.
 *
 * Encode: code/write → code_write (slashes → underscore)
 * Decode: ANY model-produced variant → original name (via reverse lookup)
 *
 * Models mangle names in unpredictable ways:
 *   code__write, $FUNCTIONS.code_write, code_write, code-write, etc.
 * The codec handles all of these by registering normalized variants at startup.
 */
export class ToolNameCodec {
  private static _instance: ToolNameCodec | null = null;
  private readonly originals: Set<string> = new Set();
  private readonly reverseMap: Map<string, string> = new Map();

  static get instance(): ToolNameCodec {
    if (!ToolNameCodec._instance) {
      ToolNameCodec._instance = new ToolNameCodec();
    }
    return ToolNameCodec._instance;
  }

  /** Register a tool name and all plausible encoded/mangled variants for reverse lookup */
  register(toolName: string): void {
    this.originals.add(toolName);
    this.reverseMap.set(toolName, toolName);

    // Canonical encoded form: slashes → single underscore (standard snake_case)
    const encoded = toolName.replace(/\//g, '_');
    this.reverseMap.set(encoded, toolName);

    // Legacy double-underscore encoding (backwards compat with old sessions)
    const doubleEncoded = toolName.replace(/\//g, '__');
    this.reverseMap.set(doubleEncoded, toolName);

    // Hyphen variant: code/write → code-write
    this.reverseMap.set(toolName.replace(/\//g, '-'), toolName);

    // Dot variant: code/write → code.write
    this.reverseMap.set(toolName.replace(/\//g, '.'), toolName);
  }

  /** Register all tool names from a tool definitions array */
  registerAll(tools: Array<{ name: string }>): void {
    for (const tool of tools) {
      this.register(tool.name);
    }
  }

  /** Encode a tool name for API transmission: slashes → underscores */
  encode(toolName: string): string {
    return toolName.replace(/\//g, '_');
  }

  /** Decode any model-produced tool name variant back to the original */
  decode(raw: string): string {
    // 1. Exact match (fastest path)
    const exact = this.reverseMap.get(raw);
    if (exact) return exact;

    // 2. Strip known prefixes models add ($FUNCTIONS., functions., $tools.)
    let cleaned = raw.replace(/^\$?(?:functions|tools)\./i, '');
    const prefixMatch = this.reverseMap.get(cleaned);
    if (prefixMatch) return prefixMatch;

    // 3. Normalize separators to underscore and try lookup
    const normalized = cleaned.replace(/[-.__]/g, '_').toLowerCase();
    const normMatch = this.reverseMap.get(normalized);
    if (normMatch) return normMatch;

    // 4. Try reconstructing with slashes: replace __ first, then remaining _
    const doubleUnderscored = cleaned.replace(/__/g, '/');
    if (this.originals.has(doubleUnderscored)) return doubleUnderscored;

    const singleUnderscored = cleaned.replace(/_/g, '/');
    if (this.originals.has(singleUnderscored)) return singleUnderscored;

    // 5. Last resort: best-effort reconstruction via double underscore
    return doubleUnderscored;
  }
}

/**
 * Convert ToolDefinition[] to NativeToolSpec[] for providers that support native JSON tools
 * (Anthropic, OpenAI, etc.)
 *
 * This enables native tool_use instead of XML parsing, which is more reliable.
 */
export function convertToNativeToolSpecs(tools: ToolDefinition[]): NativeToolSpec[] {
  // Register all tools with the codec before encoding — ensures the reverse map
  // has entries for every tool name we send to the API so decode() can resolve
  // any model-produced variant (e.g. $FUNCTIONS.code_write) back to code/write.
  ToolNameCodec.instance.registerAll(tools);

  return tools.map(tool => {
    // Convert our ToolDefinition to Anthropic's input_schema format
    const properties: Record<string, { type: string; description?: string; enum?: string[] }> = {};
    const required: string[] = [];

    for (const [paramName, paramDef] of Object.entries(tool.parameters.properties)) {
      properties[paramName] = {
        type: paramDef.type || 'string',
        description: paramDef.description,
      };

      // Check if required
      if (tool.parameters.required.includes(paramName) || paramDef.required) {
        required.push(paramName);
      }
    }

    return {
      // Sanitize name for API (data/list -> data_list)
      name: sanitizeToolName(tool.name),
      description: tool.description,
      input_schema: {
        type: 'object',
        properties,
        required: required.length > 0 ? required : undefined,
      },
    };
  });
}

/**
 * Coerce text-parsed tool parameters to match JSON Schema types from NativeToolSpec.
 * When models output tool calls in text (e.g. Groq's `<function=name>{json}</function>`),
 * values may be strings where booleans/numbers are expected. Native APIs validate
 * tool_use blocks against the schema and reject type mismatches (400 Bad Request).
 */
export function coerceParamsToSchema(
  params: Record<string, unknown>,
  toolSpecs: NativeToolSpec[],
  sanitizedToolName: string,
): Record<string, unknown> {
  const spec = toolSpecs.find(s => s.name === sanitizedToolName);
  if (!spec?.input_schema?.properties) return params;

  const coerced: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(params)) {
    const propSchema = spec.input_schema.properties[key] as { type?: string } | undefined;
    if (!propSchema?.type || typeof value !== 'string') {
      coerced[key] = value;
      continue;
    }
    switch (propSchema.type) {
      case 'boolean':
        coerced[key] = value === 'true' || value === '1';
        break;
      case 'number':
      case 'integer': {
        const num = Number(value);
        coerced[key] = isNaN(num) ? value : num;
        break;
      }
      default:
        coerced[key] = value;
    }
  }
  return coerced;
}

/**
 * Check if a provider supports native JSON tool calling.
 * OpenAI-compatible providers (Together, Groq, Fireworks, xAI) implement
 * the function calling spec (tools parameter + tool_calls in response).
 */
export function supportsNativeTools(provider: string): boolean {
  const nativeToolProviders = ['anthropic', 'openai', 'azure', 'together', 'groq', 'fireworks', 'xai'];
  return nativeToolProviders.includes(provider.toLowerCase());
}

/**
 * Tool capability tier for a given provider/model combination.
 * - 'native': JSON tool_use blocks (Anthropic, OpenAI, Azure, Together, Groq, Fireworks, xAI)
 * - 'xml': XML tool calls parsed by ToolCallParser (DeepSeek — proven to work)
 * - 'none': Model narrates instead of calling tools — don't inject tools
 */
export type ToolCapability = 'native' | 'xml' | 'none';

/**
 * Determine a model's tool-calling capability.
 * Provider-based auto-detection with per-persona override via modelConfig.toolCapability.
 *
 * IMPORTANT: Default to 'xml' not 'none'. A Candle model could be a powerful
 * fine-tuned model with LoRA. Returning 'none' leaves it completely powerless.
 * XML tool definitions are budget-aware via ToolDefinitionsSource and will be
 * truncated if the model's context is tight.
 */
export function getToolCapability(
  provider: string,
  modelConfig?: { toolCapability?: ToolCapability }
): ToolCapability {
  if (modelConfig?.toolCapability) return modelConfig.toolCapability;

  if (supportsNativeTools(provider)) return 'native';

  // All other providers get XML tool definitions in the system prompt.
  // Models that can't use them will ignore them; models that can (DeepSeek,
  // fine-tuned Candle, Ollama) benefit from having tools available.
  // Budget-aware: ToolDefinitionsSource truncates for tight context windows.
  return 'xml';
}

