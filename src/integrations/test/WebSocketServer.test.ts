/**
 * TypeScript WebSocket Server Tests
 * Verify daemon connection and command routing
 */

import { TypeScriptWebSocketServer } from '../WebSocketServer';
import WebSocket from 'ws';

describe('TypeScript WebSocket Server', () => {
    let server: TypeScriptWebSocketServer;
    let client: WebSocket;
    const testPort = 9001;

    beforeEach(async () => {
        server = new TypeScriptWebSocketServer(testPort);
        await server.start();
    });

    afterEach(async () => {
        if (client) {
            client.close();
        }
        await server.stop();
    });

    describe('Basic Server Functionality', () => {
        test('should start TypeScript WebSocket server', () => {
            expect(server).toBeDefined();
        });

        test('should accept client connections', (done) => {
            client = new WebSocket(`ws://localhost:${testPort}`);
            
            client.on('open', () => {
                expect(client.readyState).toBe(WebSocket.OPEN);
                done();
            });
        });

        test('should send connection confirmation', (done) => {
            client = new WebSocket(`ws://localhost:${testPort}`);
            
            client.on('message', (data) => {
                const message = JSON.parse(data.toString());
                
                if (message.type === 'client_connection_confirmed') {
                    expect(message.data.server).toBe('typescript-websocket');
                    expect(message.data.clientId).toBeDefined();
                    done();
                }
            });
        });
    });

    describe('Command Execution Integration', () => {
        test('should route selftest command to TypeScript daemon', (done) => {
            client = new WebSocket(`ws://localhost:${testPort}`);
            
            client.on('open', () => {
                const commandMessage = {
                    type: 'execute_command',
                    data: {
                        command: 'selftest',
                        params: '{"mode": "simple", "verbose": true}',
                        requestId: 'test-1'
                    }
                };
                
                client.send(JSON.stringify(commandMessage));
            });

            client.on('message', (data) => {
                const message = JSON.parse(data.toString());
                
                if (message.type === 'command_result') {
                    expect(message.data.command).toBe('selftest');
                    expect(message.data.result).toBeDefined();
                    expect(typeof message.data.result.success).toBe('boolean');
                    
                    // Should be processed by TypeScript daemon, not legacy
                    expect(message.data.result.processor).toBe('typescript-daemon');
                    done();
                }
            });
        });

        test('should handle devtools mode selftest', (done) => {
            client = new WebSocket(`ws://localhost:${testPort}`);
            
            client.on('open', () => {
                const commandMessage = {
                    type: 'execute_command',
                    data: {
                        command: 'selftest',
                        params: '{"mode": "devtools", "verbose": true}',
                        requestId: 'test-devtools'
                    }
                };
                
                client.send(JSON.stringify(commandMessage));
            });

            client.on('message', (data) => {
                const message = JSON.parse(data.toString());
                
                if (message.type === 'command_result') {
                    const result = message.data.result;
                    
                    // Should contain TypeScript selftest result structure
                    if (result.success) {
                        expect(result.mode).toBe('devtools'); // TypeScript version has mode
                        expect(result.tests).toBeDefined();
                        expect(result.tests.simple).toBe(true);
                    }
                    
                    done();
                }
            });
        });
    });

    describe('Error Handling', () => {
        test('should handle invalid JSON messages', (done) => {
            client = new WebSocket(`ws://localhost:${testPort}`);
            
            client.on('open', () => {
                client.send('invalid json');
            });

            client.on('message', (data) => {
                const message = JSON.parse(data.toString());
                
                if (message.type === 'error') {
                    expect(message.data.error).toContain('Invalid message format');
                    done();
                }
            });
        });

        test('should handle unknown commands gracefully', (done) => {
            client = new WebSocket(`ws://localhost:${testPort}`);
            
            client.on('open', () => {
                const commandMessage = {
                    type: 'execute_command',
                    data: {
                        command: 'nonexistent-command',
                        params: '{}',
                        requestId: 'test-unknown'
                    }
                };
                
                client.send(JSON.stringify(commandMessage));
            });

            client.on('message', (data) => {
                const message = JSON.parse(data.toString());
                
                if (message.type === 'command_result') {
                    expect(message.data.result.success).toBe(false);
                    expect(message.data.result.error).toBeDefined();
                    done();
                }
            });
        });
    });

    describe('Daemon Connection', () => {
        test('should report daemon connection status', (done) => {
            client = new WebSocket(`ws://localhost:${testPort}`);
            
            client.on('open', () => {
                const initMessage = {
                    type: 'client_init',
                    data: { client: 'test' }
                };
                
                client.send(JSON.stringify(initMessage));
            });

            client.on('message', (data) => {
                const message = JSON.parse(data.toString());
                
                if (message.type === 'init_response') {
                    expect(message.data.server).toBe('typescript-websocket');
                    expect(message.data.daemon).toBeDefined();
                    expect(['connected', 'disconnected'].includes(message.data.daemon)).toBe(true);
                    expect(Array.isArray(message.data.capabilities)).toBe(true);
                    done();
                }
            });
        });
    });
});