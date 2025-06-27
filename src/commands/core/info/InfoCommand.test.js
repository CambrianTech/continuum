/**
 * InfoCommand TypeScript Unit Tests
 * Comprehensive test coverage for system information display
 */
import { InfoCommand } from './InfoCommand';
import * as fs from 'fs';
import * as os from 'os';
// Mock dependencies
jest.mock('fs');
jest.mock('os');
const mockedFs = fs;
const mockedOs = os;
describe('InfoCommand', () => {
    let mockContext;
    let mockWebSocketServer;
    beforeEach(() => {
        // Setup mocks
        mockWebSocketServer = {
            clients: { size: 3 }
        };
        mockContext = {
            webSocketServer: mockWebSocketServer
        };
        // Mock OS functions
        mockedOs.platform.mockReturnValue('darwin');
        mockedOs.arch.mockReturnValue('x64');
        mockedOs.type.mockReturnValue('Darwin');
        mockedOs.release.mockReturnValue('21.0.0');
        mockedOs.uptime.mockReturnValue(7200); // 2 hours
        mockedOs.cpus.mockReturnValue([
            { model: 'Intel Core i7' },
            { model: 'Intel Core i7' }
        ]);
        // Mock process properties
        Object.defineProperty(process, 'version', { value: 'v18.0.0', writable: true });
        Object.defineProperty(process, 'uptime', { value: () => 3600, writable: true }); // 1 hour
        Object.defineProperty(process, 'pid', { value: 12345, writable: true });
        Object.defineProperty(process, 'cwd', { value: () => '/test/directory', writable: true });
        Object.defineProperty(process, 'argv', { value: ['node', 'script.js', '--verbose'], writable: true });
        Object.defineProperty(process, 'memoryUsage', {
            value: () => ({
                rss: 100 * 1024 * 1024,
                heapUsed: 50 * 1024 * 1024,
                heapTotal: 80 * 1024 * 1024,
                external: 10 * 1024 * 1024
            }),
            writable: true
        });
        // Mock package.json
        mockedFs.readFileSync.mockImplementation((path) => {
            if (path.includes('package.json')) {
                return JSON.stringify({ version: '1.0.0' });
            }
            if (path.includes('README.md')) {
                return `
## Definition
**Name**: info
**Description**: Display system information
**Icon**: ℹ️
**Category**: system
        `;
            }
            throw new Error('File not found');
        });
        // Mock console.log to prevent test output noise
        jest.spyOn(console, 'log').mockImplementation();
    });
    afterEach(() => {
        jest.restoreAllMocks();
    });
    describe('Command Definition', () => {
        test('should provide valid command definition', () => {
            const definition = InfoCommand.getDefinition();
            expect(definition).toBeDefined();
            expect(definition.name).toBe('info');
            expect(definition.category).toBe('system');
            expect(definition.icon).toBe('ℹ️');
            expect(definition.description).toContain('system information');
        });
        test('should use fallback definition when README.md not found', () => {
            mockedFs.readFileSync.mockImplementation(() => {
                throw new Error('File not found');
            });
            const definition = InfoCommand.getDefinition();
            expect(definition.name).toBe('info');
            expect(definition.description).toBe('Display system information and server status');
            expect(definition.examples).toHaveLength(3);
        });
    });
    describe('Parameter Handling', () => {
        test('should handle no parameters (default to overview)', async () => {
            const result = await InfoCommand.execute({}, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('system');
            expect(result.data).toHaveProperty('server');
        });
        test('should handle JSON string parameters', async () => {
            const jsonParams = JSON.stringify({ section: 'system' });
            const result = await InfoCommand.execute(jsonParams, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('system');
            expect(result.data).not.toHaveProperty('server');
        });
        test('should handle object parameters', async () => {
            const result = await InfoCommand.execute({ section: 'memory' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('memory');
        });
        test('should reject invalid section', async () => {
            const result = await InfoCommand.execute({ section: 'invalid' }, mockContext);
            expect(result.success).toBe(false);
            expect(result.error).toContain('Invalid section: invalid');
        });
    });
    describe('Section Display', () => {
        test('should display overview section', async () => {
            const result = await InfoCommand.execute({ section: 'overview' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('system');
            expect(result.data).toHaveProperty('server');
            expect(result.data).toHaveProperty('version');
        });
        test('should display system section only', async () => {
            const result = await InfoCommand.execute({ section: 'system' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('system');
            expect(result.data).not.toHaveProperty('server');
            expect(result.data.system).toMatchObject({
                platform: 'darwin',
                architecture: 'x64',
                nodeVersion: 'v18.0.0',
                osType: 'Darwin',
                osRelease: '21.0.0',
                cpuModel: 'Intel Core i7',
                cpuCores: 2,
                uptime: expect.stringContaining('h')
            });
        });
        test('should display server section only', async () => {
            const result = await InfoCommand.execute({ section: 'server' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('server');
            expect(result.data).not.toHaveProperty('system');
            expect(result.data.server).toMatchObject({
                version: '1.0.0',
                uptime: expect.stringContaining('h'),
                pid: 12345,
                workingDirectory: '/test/directory',
                nodeArgs: '--verbose'
            });
        });
        test('should display memory section only', async () => {
            const result = await InfoCommand.execute({ section: 'memory' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('memory');
            expect(result.data.memory).toMatchObject({
                rss: '100.00 MB',
                heapUsed: '50.00 MB',
                heapTotal: '80.00 MB',
                external: '10.00 MB'
            });
        });
        test('should display connections section only', async () => {
            const result = await InfoCommand.execute({ section: 'connections' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data).toHaveProperty('connections');
            expect(result.data.connections).toMatchObject({
                webSocketServer: 'Active',
                port: 9000,
                activeConnections: 3,
                commandBus: 'Ready'
            });
        });
    });
    describe('Type Safety', () => {
        test('should maintain type safety throughout execution', async () => {
            const result = await InfoCommand.execute({ section: 'overview' }, mockContext);
            expect(result.success).toBe(true);
            expect(typeof result.message).toBe('string');
            expect(typeof result.data?.version).toBe('string');
            expect(typeof result.data?.system?.cpuCores).toBe('number');
            expect(typeof result.data?.server?.pid).toBe('number');
        });
        test('should handle missing context gracefully', async () => {
            const result = await InfoCommand.execute({ section: 'connections' });
            expect(result.success).toBe(true);
            expect(result.data?.connections?.activeConnections).toBe(0);
        });
    });
    describe('Error Handling', () => {
        test('should handle version reading errors gracefully', async () => {
            mockedFs.readFileSync.mockImplementation((path) => {
                if (path.includes('package.json')) {
                    throw new Error('Package.json not found');
                }
                throw new Error('File not found');
            });
            const result = await InfoCommand.execute({ section: 'server' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data?.server?.version).toBe('unknown');
        });
        test('should handle general execution errors', async () => {
            // Force an error by mocking os.platform to throw
            mockedOs.platform.mockImplementation(() => {
                throw new Error('OS error');
            });
            const result = await InfoCommand.execute({ section: 'system' }, mockContext);
            expect(result.success).toBe(false);
            expect(result.error).toContain('Failed to get system information');
        });
    });
    describe('Formatting', () => {
        test('should format memory usage correctly', async () => {
            const result = await InfoCommand.execute({ section: 'memory' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data?.memory?.rss).toMatch(/^\d+\.\d{2} MB$/);
            expect(result.data?.memory?.heapUsed).toMatch(/^\d+\.\d{2} MB$/);
        });
        test('should format uptime correctly', async () => {
            const result = await InfoCommand.execute({ section: 'system' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data?.system?.uptime).toMatch(/^\d+h \d+m$/);
        });
        test('should format server uptime with seconds', async () => {
            const result = await InfoCommand.execute({ section: 'server' }, mockContext);
            expect(result.success).toBe(true);
            expect(result.data?.server?.uptime).toMatch(/^\d+h \d+m \d+s$/);
        });
    });
    describe('Console Output', () => {
        test('should call console.log for display sections', async () => {
            const consoleSpy = jest.spyOn(console, 'log');
            await InfoCommand.execute({ section: 'system' }, mockContext);
            expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('🖥️ SYSTEM'));
            expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Platform:'));
        });
    });
});
//# sourceMappingURL=InfoCommand.test.js.map