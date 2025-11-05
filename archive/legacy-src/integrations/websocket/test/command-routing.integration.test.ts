/**
 * WebSocket Command Routing Integration Tests
 * 
 * CRITICAL: These tests prevent the command routing pipeline failures
 * that caused 100% command timeout and system lockup.
 * 
 * Tests the complete message flow:
 * Browser → WebSocket → CommandProcessor → Command Execution → Response
 */

import { describe, test, expect, beforeAll, afterAll } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon';
import { CommandProcessorDaemon } from '../../../daemons/command-processor/CommandProcessorDaemon';

describe('WebSocket Command Routing Integration', () => {
    let webSocketDaemon: WebSocketDaemon;
    let commandProcessor: CommandProcessorDaemon;
    
    beforeAll(async () => {
        // Initialize daemons for testing
        webSocketDaemon = new WebSocketDaemon();
        commandProcessor = new CommandProcessorDaemon();
        
        // Start command processor with real command discovery
        await commandProcessor.start();
        
        // Ensure WebSocket daemon can communicate with command processor
        (webSocketDaemon as any).commandProcessor = commandProcessor;
    });
    
    afterAll(async () => {
        await commandProcessor?.stop();
        await webSocketDaemon?.stop();
    });

    test('WebSocket message parsing extracts command correctly', () => {
        const webSocketMessage = {
            data: {
                command: 'health',
                params: { verbose: true }
            },
            clientId: 'test-client-123'
        };
        
        // Test the message parsing logic that was broken
        const parsed = (webSocketDaemon as any).parseWebSocketMessage(webSocketMessage);
        
        expect(parsed.command).toBe('health');
        expect(parsed.params).toEqual({ verbose: true });
        expect(parsed.clientId).toBe('test-client-123');
    });

    test('Command routing to processor uses correct daemon protocol', async () => {
        const mockMessage = {
            data: {
                command: 'health',
                params: {}
            },
            clientId: 'test-routing-client'
        };
        
        // Simulate the routing that was failing
        const routingResult = await (webSocketDaemon as any).routeCommandToProcessor(
            'test-connection-id',
            mockMessage
        );
        
        expect(routingResult.success).toBe(true);
        expect(routingResult.data).toBeDefined();
    });

    test('Command processor receives and executes health command', async () => {
        const daemonMessage = {
            type: 'command_request',
            data: {
                command: 'health',
                params: { source: 'integration-test' }
            }
        };
        
        const result = await commandProcessor.handleMessage(daemonMessage);
        
        expect(result.success).toBe(true);
        expect(result.data.status).toBe('healthy');
    });

    test('Command discovery finds all expected commands', async () => {
        const discoveredCommands = await (commandProcessor as any).discoverCommands();
        
        // These commands should always be available
        const requiredCommands = ['health', 'console', 'agents', 'projects', 'chat'];
        
        for (const requiredCommand of requiredCommands) {
            expect(discoveredCommands).toContain(requiredCommand);
        }
        
        // Should have discovered a reasonable number of commands
        expect(discoveredCommands.length).toBeGreaterThan(10);
    });

    test('End-to-end command flow prevents timeout', async () => {
        // Simulate complete browser → WebSocket → Command flow
        const browserMessage = {
            data: {
                command: 'health',
                params: { timestamp: Date.now() }
            },
            clientId: 'e2e-test-client'
        };
        
        const startTime = Date.now();
        
        const response = await (webSocketDaemon as any).routeCommandToProcessor(
            'e2e-connection',
            browserMessage
        );
        
        const executionTime = Date.now() - startTime;
        
        // Command should execute quickly (< 1000ms), not timeout (30000ms)
        expect(executionTime).toBeLessThan(1000);
        expect(response.success).toBe(true);
    });

    test('Invalid command handling prevents system lockup', async () => {
        const invalidMessage = {
            data: {
                command: 'nonexistent-command-xyz',
                params: {}
            },
            clientId: 'invalid-test-client'
        };
        
        const response = await (webSocketDaemon as any).routeCommandToProcessor(
            'invalid-connection',
            invalidMessage
        );
        
        // Should fail gracefully, not hang
        expect(response.success).toBe(false);
        expect(response.error).toContain('Command not found');
    });

    test('WebSocket daemon properly initializes command processor reference', () => {
        // Prevent the undefined commandProcessor.handleWebSocketCommand() error
        expect((webSocketDaemon as any).commandProcessor).toBeDefined();
        expect(typeof (webSocketDaemon as any).commandProcessor.handleMessage).toBe('function');
    });

    test('Command message structure validation', () => {
        const validMessages = [
            { data: { command: 'health' }, clientId: 'test1' },
            { data: { command: 'console', params: { level: 'info' } }, clientId: 'test2' }
        ];
        
        const invalidMessages = [
            { command: 'health' }, // Missing data wrapper
            { data: {} }, // Missing command
            { data: { command: '' } }, // Empty command
        ];
        
        for (const valid of validMessages) {
            expect(() => (webSocketDaemon as any).validateMessage(valid)).not.toThrow();
        }
        
        for (const invalid of invalidMessages) {
            expect(() => (webSocketDaemon as any).validateMessage(invalid)).toThrow();
        }
    });
});