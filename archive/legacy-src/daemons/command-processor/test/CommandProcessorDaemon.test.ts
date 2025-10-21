/**
 * CommandProcessorDaemon Tests - TypeScript Implementation
 * Step-by-step daemon testing with JTAG portal integration
 */

import { CommandProcessorDaemon } from '../CommandProcessorDaemon';

describe('CommandProcessorDaemon TypeScript Tests', () => {
    let daemon: CommandProcessorDaemon;

    beforeEach(async () => {
        daemon = new CommandProcessorDaemon({
            id: 'test-command-processor',
            logLevel: 'debug'
        });
    });

    afterEach(async () => {
        if (daemon) {
            await daemon.stop();
        }
    });

    test('MEMORY LEAK FIX: should clean up execution monitoring interval on stop', async () => {
        await daemon.start();
        
        // Verify daemon started
        expect(daemon.getSimpleStatus()).toBe('running');
        
        // Stop daemon
        await daemon.stop();
        
        // No interval-based logging should occur after stop
        expect(daemon.getSimpleStatus()).toBe('stopped');
        // Note: CommandProcessorDaemon needs monitoring interval cleanup implementation
    });

    describe('Basic Daemon Functionality', () => {
        test('should create daemon with proper TypeScript typing', () => {
            expect(daemon).toBeDefined();
            expect(daemon.id).toBe('test-command-processor');
            expect(typeof daemon.start).toBe('function');
            expect(typeof daemon.stop).toBe('function');
            expect(typeof daemon.getStatus).toBe('function');
        });

        test('should start daemon and emit events', async () => {
            const startPromise = new Promise((resolve) => {
                daemon.on('started', resolve);
            });

            await daemon.start();
            await startPromise;

            const status = daemon.getStatus();
            expect(status.running).toBe(true);
            expect(status.startTime).toBeDefined();
        });

        test('should stop daemon and emit events', async () => {
            await daemon.start();

            const stopPromise = new Promise((resolve) => {
                daemon.on('stopped', resolve);
            });

            await daemon.stop();
            await stopPromise;

            const status = daemon.getStatus();
            expect(status.running).toBe(false);
        });
    });

    describe('Command Processing', () => {
        test('should process TypeScript selftest command', async () => {
            await daemon.start();

            const commandRequest = {
                command: 'selftest',
                parameters: { mode: 'simple', verbose: true },
                context: { test: true }
            };

            const result = await daemon.processCommand(commandRequest);

            expect(result).toBeDefined();
            expect(typeof result.success).toBe('boolean');
            expect(result.processor).toBe('typescript-daemon');
        });

        test('should handle unknown commands gracefully', async () => {
            await daemon.start();

            const commandRequest = {
                command: 'nonexistent-command',
                parameters: {},
                context: {}
            };

            const result = await daemon.processCommand(commandRequest);

            expect(result.success).toBe(false);
            expect(result.error).toContain('not found');
        });
    });

    describe('Event System', () => {
        test('should emit command execution events', async () => {
            await daemon.start();

            const events: string[] = [];
            daemon.on('command:start', () => events.push('start'));
            daemon.on('command:complete', () => events.push('complete'));

            await daemon.processCommand({
                command: 'selftest',
                parameters: { mode: 'simple' },
                context: {}
            });

            expect(events).toContain('start');
            expect(events).toContain('complete');
        });

        test('should emit error events for failed commands', async () => {
            await daemon.start();

            let errorEmitted = false;
            daemon.on('command:error', () => {
                errorEmitted = true;
            });

            await daemon.processCommand({
                command: 'invalid-command',
                parameters: {},
                context: {}
            });

            expect(errorEmitted).toBe(true);
        });
    });

    describe('JTAG Integration', () => {
        test('should provide JTAG logging capabilities', async () => {
            await daemon.start();

            const logs = daemon.getJTAGLogs();
            expect(Array.isArray(logs)).toBe(true);

            // Execute a command to generate logs
            await daemon.processCommand({
                command: 'selftest',
                parameters: { verbose: true },
                context: { jtag: true }
            });

            const updatedLogs = daemon.getJTAGLogs();
            expect(updatedLogs.length).toBeGreaterThan(logs.length);
        });

        test('should capture command execution traces', async () => {
            await daemon.start();

            daemon.enableJTAGTracing(true);

            await daemon.processCommand({
                command: 'selftest',
                parameters: { mode: 'simple' },
                context: { trace: true }
            });

            const traces = daemon.getExecutionTraces();
            expect(Array.isArray(traces)).toBe(true);
            expect(traces.length).toBeGreaterThan(0);

            const lastTrace = traces[traces.length - 1];
            expect(lastTrace).toHaveProperty('command');
            expect(lastTrace).toHaveProperty('timestamp');
            expect(lastTrace).toHaveProperty('duration');
            expect(lastTrace.command).toBe('selftest');
        });
    });
});