/**
 * WebMCP Shared Types - Chrome 146+ WebMCP API types
 *
 * Shared across all WebMCP commands to avoid duplicate declarations.
 */

/**
 * WebMCP Tool definition - matches Chrome 146+ WebMCP spec
 */
export interface WebMCPTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

/**
 * WebMCP Tool definition with execute function (for registration)
 */
export interface WebMCPToolDefinition extends WebMCPTool {
  execute: (params: unknown) => Promise<unknown>;
}

/**
 * WebMCP Model Context API - Chrome 146+ navigator.modelContext
 */
export interface WebMCPModelContext {
  registerTool: (tool: WebMCPToolDefinition) => void;
  unregisterTool: (name: string) => void;
  provideContext: (tools: WebMCPToolDefinition[]) => void;
  clearContext: () => void;
  // Non-standard extension: query registered tools (may not exist in spec)
  getTools?: () => WebMCPToolDefinition[];
}

/**
 * Extend Navigator for WebMCP API (Chrome 146+)
 * Only declare once to avoid TypeScript conflicts.
 */
declare global {
  interface Navigator {
    modelContext?: WebMCPModelContext;
  }

  interface Window {
    __webmcp_tools?: WebMCPToolDefinition[];
  }
}

/**
 * Check if WebMCP is available in the current browser context
 */
export function checkWebMCPAvailability(): { available: boolean; reason: string } {
  if (typeof navigator === 'undefined') {
    return {
      available: false,
      reason: 'Not in browser context',
    };
  }

  if (!navigator.modelContext) {
    return {
      available: false,
      reason: 'Chrome Canary 146+ with WebMCP flag required. Enable at chrome://flags/#web-mcp-for-testing',
    };
  }

  if (typeof navigator.modelContext.registerTool !== 'function') {
    return {
      available: false,
      reason: 'WebMCP API present but incomplete - registerTool not found',
    };
  }

  return {
    available: true,
    reason: '',
  };
}

/**
 * Convert FormData to object (handles environments where FormData.entries() may not exist)
 */
export function formDataToObject(formData: FormData): Record<string, string> {
  const result: Record<string, string> = {};
  // Use forEach which is more widely supported than entries()
  formData.forEach((value, key) => {
    result[key] = String(value);
  });
  return result;
}
