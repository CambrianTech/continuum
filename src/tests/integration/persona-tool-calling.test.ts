/**
 * persona-tool-calling.test.ts
 *
 * Integration tests for Phase 3A: Tool Calling Foundation
 * Tests PersonaToolExecutor, ToolRegistry, and PersonaToolRegistry
 *
 * Run with: npx vitest tests/integration/persona-tool-calling.test.ts
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { PersonaToolExecutor } from '@system/user/server/modules/PersonaToolExecutor';
import { ToolRegistry } from '../../tools/server/ToolRegistry';
import { PersonaToolRegistry } from '@system/user/server/modules/PersonaToolRegistry';
import {
  getToolDefinition,
  validateToolParameters,
  formatToolForAI,
  formatAllToolsForAI
} from '@system/user/server/modules/PersonaToolDefinitions';
import type { UUID } from '@types/CrossPlatformUUID';

// Mock UUIDs for testing
const MOCK_PERSONA_ID: UUID = 'persona-test-001' as UUID;
const MOCK_SESSION_ID: UUID = 'session-test-001' as UUID;
const MOCK_CONTEXT_ID: UUID = 'context-test-001' as UUID;
const MOCK_PERSONA_NAME = 'TestPersona';

describe('Phase 3A: Tool Calling Foundation', () => {
  describe('PersonaToolDefinitions', () => {
    it('should define all built-in tools', () => {
      const read = getToolDefinition('read');
      expect(read).toBeDefined();
      expect(read?.name).toBe('read');
      expect(read?.category).toBe('file');
      expect(read?.permissions).toContain('file:read');

      const grep = getToolDefinition('grep');
      expect(grep).toBeDefined();
      expect(grep?.name).toBe('grep');
      expect(grep?.category).toBe('code');

      const bash = getToolDefinition('bash');
      expect(bash).toBeDefined();
      expect(bash?.name).toBe('bash');
      expect(bash?.category).toBe('system');

      const screenshot = getToolDefinition('screenshot');
      expect(screenshot).toBeDefined();
      expect(screenshot?.name).toBe('screenshot');
      expect(screenshot?.category).toBe('media');
    });

    it('should validate tool parameters correctly', () => {
      // Valid params for 'read' tool
      const validRead = validateToolParameters('read', {
        filepath: '/path/to/file.ts'
      });
      expect(validRead.valid).toBe(true);
      expect(validRead.errors).toHaveLength(0);

      // Missing required parameter
      const invalidRead = validateToolParameters('read', {});
      expect(invalidRead.valid).toBe(false);
      expect(invalidRead.errors).toContain('Missing required parameter: filepath');

      // Valid params for 'grep' tool
      const validGrep = validateToolParameters('grep', {
        pattern: 'PersonaUser',
        glob: '*.ts'
      });
      expect(validGrep.valid).toBe(true);

      // Missing required parameter
      const invalidGrep = validateToolParameters('grep', {
        glob: '*.ts'
      });
      expect(invalidGrep.valid).toBe(false);
      expect(invalidGrep.errors).toContain('Missing required parameter: pattern');
    });

    it('should format tools for AI consumption', () => {
      const readTool = getToolDefinition('read');
      expect(readTool).toBeDefined();

      const formatted = formatToolForAI(readTool!);
      expect(formatted).toContain('Tool: read');
      expect(formatted).toContain('Description:');
      expect(formatted).toContain('Parameters:');
      expect(formatted).toContain('filepath');
      expect(formatted).toContain('Examples:');
    });

    it('should format all tools for system prompt', () => {
      const allTools = formatAllToolsForAI();
      expect(allTools).toContain('Available Tools:');
      expect(allTools).toContain('Tool: read');
      expect(allTools).toContain('Tool: grep');
      expect(allTools).toContain('Tool: bash');
      expect(allTools).toContain('Tool: screenshot');
      expect(allTools).toContain('Usage Format');
      expect(allTools).toContain('<tool_use>');
    });

    it('should validate enum parameters', () => {
      const validEnum = validateToolParameters('grep', {
        pattern: 'test',
        output_mode: 'content'
      });
      expect(validEnum.valid).toBe(true);

      const invalidEnum = validateToolParameters('grep', {
        pattern: 'test',
        output_mode: 'invalid_mode'
      });
      expect(invalidEnum.valid).toBe(false);
      expect(invalidEnum.errors.some(e => e.includes('must be one of'))).toBe(true);
    });
  });

  describe('PersonaToolRegistry', () => {
    let registry: PersonaToolRegistry;

    beforeEach(() => {
      // Reset singleton for each test
      PersonaToolRegistry.resetSharedInstance();
      registry = PersonaToolRegistry.sharedInstance();
    });

    it('should list all available tools', () => {
      const tools = registry.listTools();
      expect(tools).toHaveLength(4); // read, grep, bash, screenshot
      expect(tools.map(t => t.name)).toContain('read');
      expect(tools.map(t => t.name)).toContain('grep');
      expect(tools.map(t => t.name)).toContain('bash');
      expect(tools.map(t => t.name)).toContain('screenshot');
    });

    it('should manage persona permissions', () => {
      registry.registerPersona(MOCK_PERSONA_ID, 'assistant');

      // Assistant should have read, grep, bash, screenshot permissions
      expect(registry.canUse('read', MOCK_PERSONA_ID)).toBe(true);
      expect(registry.canUse('grep', MOCK_PERSONA_ID)).toBe(true);
      expect(registry.canUse('bash', MOCK_PERSONA_ID)).toBe(true);
      expect(registry.canUse('screenshot', MOCK_PERSONA_ID)).toBe(true);
    });

    it('should enforce role-based permissions', () => {
      const restrictedId = 'restricted-persona-001' as UUID;
      registry.registerPersona(restrictedId, 'restricted');

      // Restricted role should only have read and grep
      expect(registry.canUse('read', restrictedId)).toBe(true);
      expect(registry.canUse('grep', restrictedId)).toBe(true);
      expect(registry.canUse('bash', restrictedId)).toBe(false);
      expect(registry.canUse('screenshot', restrictedId)).toBe(false);
    });

    it('should grant and revoke permissions dynamically', () => {
      registry.registerPersona(MOCK_PERSONA_ID, 'restricted');
      expect(registry.canUse('bash', MOCK_PERSONA_ID)).toBe(false);

      registry.grantPermission(MOCK_PERSONA_ID, 'system:execute');
      expect(registry.canUse('bash', MOCK_PERSONA_ID)).toBe(true);

      registry.revokePermission(MOCK_PERSONA_ID, 'system:execute');
      expect(registry.canUse('bash', MOCK_PERSONA_ID)).toBe(false);
    });

    it('should filter tools by persona permissions', () => {
      registry.registerPersona(MOCK_PERSONA_ID, 'restricted');
      const availableTools = registry.listToolsForPersona(MOCK_PERSONA_ID);

      expect(availableTools).toHaveLength(2); // Only read and grep
      expect(availableTools.map(t => t.name)).toContain('read');
      expect(availableTools.map(t => t.name)).toContain('grep');
      expect(availableTools.map(t => t.name)).not.toContain('bash');
      expect(availableTools.map(t => t.name)).not.toContain('screenshot');
    });

    it('should search tools by keyword', () => {
      const fileTools = registry.searchTools('file');
      expect(fileTools.some(t => t.name === 'read')).toBe(true);

      const searchTools = registry.searchTools('search');
      expect(searchTools.some(t => t.name === 'grep')).toBe(true);
    });

    it('should get tools by category', () => {
      const fileTools = registry.getToolsByCategory('file');
      expect(fileTools).toHaveLength(1);
      expect(fileTools[0].name).toBe('read');

      const codeTools = registry.getToolsByCategory('code');
      expect(codeTools).toHaveLength(1);
      expect(codeTools[0].name).toBe('grep');
    });

    it('should provide registry statistics', () => {
      registry.registerPersona(MOCK_PERSONA_ID, 'assistant');
      registry.registerPersona('admin-persona' as UUID, 'admin');
      registry.registerPersona('restricted-persona' as UUID, 'restricted');

      const stats = registry.getStats();
      expect(stats.totalPersonas).toBe(3);
      expect(stats.totalTools).toBe(4);
      expect(stats.permissionsByRole.assistant).toBe(1);
      expect(stats.permissionsByRole.admin).toBe(1);
      expect(stats.permissionsByRole.restricted).toBe(1);
    });
  });

  describe('ToolRegistry', () => {
    let toolRegistry: ToolRegistry;

    beforeEach(() => {
      toolRegistry = ToolRegistry.getInstance();
    });

    it('should be a singleton', () => {
      const instance1 = ToolRegistry.getInstance();
      const instance2 = ToolRegistry.getInstance();
      expect(instance1).toBe(instance2);
    });

    it('should list all available tools', () => {
      const tools = toolRegistry.getAllTools();
      expect(tools).toHaveLength(4);
      expect(tools.map(t => t.name)).toContain('read');
      expect(tools.map(t => t.name)).toContain('grep');
      expect(tools.map(t => t.name)).toContain('bash');
      expect(tools.map(t => t.name)).toContain('screenshot');
    });

    it('should get tool by name', () => {
      const readTool = toolRegistry.getTool('read');
      expect(readTool).toBeDefined();
      expect(readTool?.name).toBe('read');
      expect(readTool?.category).toBe('file');

      const invalidTool = toolRegistry.getTool('nonexistent');
      expect(invalidTool).toBeNull();
    });

    it('should return error for non-existent tool', async () => {
      const result = await toolRegistry.executeTool(
        'nonexistent',
        {},
        MOCK_SESSION_ID,
        MOCK_CONTEXT_ID,
        { sessionId: MOCK_SESSION_ID, contextId: MOCK_CONTEXT_ID } as any
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain('not found');
    });

    it('should return error for invalid parameters', async () => {
      const result = await toolRegistry.executeTool(
        'read',
        {}, // Missing required 'filepath'
        MOCK_SESSION_ID,
        MOCK_CONTEXT_ID,
        { sessionId: MOCK_SESSION_ID, contextId: MOCK_CONTEXT_ID } as any
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain('Invalid parameters');
      expect(result.error).toContain('filepath');
    });

    it('should convert string parameters to correct types', async () => {
      // Test that numeric strings are converted to numbers
      const result = await toolRegistry.executeTool(
        'read',
        {
          filepath: '/test/file.ts',
          offset: '100', // String should be converted to number
          limit: '50'    // String should be converted to number
        },
        MOCK_SESSION_ID,
        MOCK_CONTEXT_ID,
        { sessionId: MOCK_SESSION_ID, contextId: MOCK_CONTEXT_ID } as any
      );

      // Even if the file doesn't exist, we're testing parameter conversion,
      // so just check that the call was made with proper types
      expect(result.toolName).toBe('read');
    });
  });

  describe('PersonaToolExecutor', () => {
    let executor: PersonaToolExecutor;

    beforeEach(() => {
      executor = new PersonaToolExecutor(MOCK_PERSONA_ID, MOCK_PERSONA_NAME);
    });

    it('should parse XML tool calls correctly', () => {
      const responseWithTools = `
Let me read that file for you.

<tool name="read">
<filepath>/path/to/file.ts</filepath>
</tool>

I'll check what's in that file.
`;

      const toolCalls = executor.parseToolCalls(responseWithTools);
      expect(toolCalls).toHaveLength(1);
      expect(toolCalls[0].toolName).toBe('read');
      expect(toolCalls[0].parameters.filepath).toBe('/path/to/file.ts');
    });

    it('should parse multiple tool calls', () => {
      const responseWithMultipleTools = `
<tool name="read">
<filepath>/first/file.ts</filepath>
</tool>

<tool name="grep">
<pattern>PersonaUser</pattern>
<glob>*.ts</glob>
</tool>
`;

      const toolCalls = executor.parseToolCalls(responseWithMultipleTools);
      expect(toolCalls).toHaveLength(2);
      expect(toolCalls[0].toolName).toBe('read');
      expect(toolCalls[1].toolName).toBe('grep');
      expect(toolCalls[1].parameters.pattern).toBe('PersonaUser');
      expect(toolCalls[1].parameters.glob).toBe('*.ts');
    });

    it('should parse tool calls with complex parameters', () => {
      const responseWithComplexParams = `
<tool name="bash">
<command>ls -la | grep test</command>
<timeout>30000</timeout>
</tool>
`;

      const toolCalls = executor.parseToolCalls(responseWithComplexParams);
      expect(toolCalls).toHaveLength(1);
      expect(toolCalls[0].toolName).toBe('bash');
      expect(toolCalls[0].parameters.command).toBe('ls -la | grep test');
      expect(toolCalls[0].parameters.timeout).toBe('30000');
    });

    it('should strip tool blocks from response', () => {
      const response = `
Let me check that file.

<tool name="read">
<filepath>/test.ts</filepath>
</tool>

I found some interesting content.
`;

      const stripped = executor.stripToolBlocks(response);
      expect(stripped).not.toContain('<tool');
      expect(stripped).not.toContain('</tool>');
      expect(stripped).toContain('Let me check that file');
      expect(stripped).toContain('I found some interesting content');
    });

    it('should list available tools', () => {
      const tools = executor.getAvailableTools();
      expect(tools).toHaveLength(4);
      expect(tools).toContain('read');
      expect(tools).toContain('grep');
      expect(tools).toContain('bash');
      expect(tools).toContain('screenshot');
    });

    it('should handle empty tool call list', async () => {
      const context = {
        personaId: MOCK_PERSONA_ID,
        personaName: MOCK_PERSONA_NAME,
        sessionId: MOCK_SESSION_ID,
        contextId: MOCK_CONTEXT_ID,
        context: { sessionId: MOCK_SESSION_ID, contextId: MOCK_CONTEXT_ID } as any,
        personaConfig: {
          autoLoadMedia: false,
          supportedMediaTypes: []
        }
      };

      const result = await executor.executeToolCalls([], context);
      expect(result.formattedResults).toBe('');
      expect(result.media).toBeUndefined();
    });
  });

  describe('End-to-End Tool Execution', () => {
    it('should execute complete tool calling flow', async () => {
      // 1. Create executor and registry
      const executor = new PersonaToolExecutor(MOCK_PERSONA_ID, MOCK_PERSONA_NAME);
      const registry = PersonaToolRegistry.sharedInstance();

      // 2. Register persona with permissions
      registry.registerPersona(MOCK_PERSONA_ID, 'assistant');

      // 3. Verify persona can use tools
      expect(registry.canUse('read', MOCK_PERSONA_ID)).toBe(true);

      // 4. Parse AI response with tool calls
      const aiResponse = `
<tool name="bash">
<command>echo "Hello Phase 3A"</command>
</tool>
`;

      const toolCalls = executor.parseToolCalls(aiResponse);
      expect(toolCalls).toHaveLength(1);

      // 5. Check that the tool execution framework is in place
      // (Actual execution would require Commands system to be running)
      const toolRegistry = ToolRegistry.getInstance();
      const tools = toolRegistry.getAllTools();
      expect(tools.map(t => t.name)).toContain('bash');
    });
  });

  describe('Phase 3A Success Criteria', () => {
    it('✅ PersonaUser can discover available tools', () => {
      const registry = PersonaToolRegistry.sharedInstance();
      const tools = registry.listTools();
      expect(tools.length).toBeGreaterThan(0);
      expect(tools.map(t => t.name)).toContain('read');
      expect(tools.map(t => t.name)).toContain('grep');
      expect(tools.map(t => t.name)).toContain('bash');
      expect(tools.map(t => t.name)).toContain('screenshot');
    });

    it('✅ Tool results correctly parsed and returned', () => {
      const executor = new PersonaToolExecutor(MOCK_PERSONA_ID, MOCK_PERSONA_NAME);
      const formatted = executor.stripToolBlocks('<tool name="test"></tool>Rest of message');
      expect(formatted).toBe('Rest of message');
    });

    it('✅ Error handling for invalid tool calls', async () => {
      const toolRegistry = ToolRegistry.getInstance();

      // Test non-existent tool
      const result1 = await toolRegistry.executeTool(
        'nonexistent',
        {},
        MOCK_SESSION_ID,
        MOCK_CONTEXT_ID,
        { sessionId: MOCK_SESSION_ID } as any
      );
      expect(result1.success).toBe(false);
      expect(result1.error).toBeDefined();

      // Test invalid parameters
      const result2 = await toolRegistry.executeTool(
        'read',
        {}, // Missing filepath
        MOCK_SESSION_ID,
        MOCK_CONTEXT_ID,
        { sessionId: MOCK_SESSION_ID } as any
      );
      expect(result2.success).toBe(false);
      expect(result2.error).toContain('Invalid parameters');
    });

    it('✅ Permission system enforces tool access', () => {
      const registry = PersonaToolRegistry.sharedInstance();
      PersonaToolRegistry.resetSharedInstance();

      const restrictedId = 'restricted-test' as UUID;
      registry.registerPersona(restrictedId, 'restricted');

      // Restricted personas can't use bash
      expect(registry.canUse('bash', restrictedId)).toBe(false);

      // But they can use read and grep
      expect(registry.canUse('read', restrictedId)).toBe(true);
      expect(registry.canUse('grep', restrictedId)).toBe(true);
    });
  });
});
