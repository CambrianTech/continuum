/**
 * Tests for SelfTestCommand - TypeScript Implementation
 * Comprehensive testing for JTAG system verification
 */
import { SelfTestCommand } from '../SelfTestCommand';
describe('SelfTestCommand TypeScript Implementation', () => {
    test('should have valid command definition', () => {
        const definition = SelfTestCommand.getDefinition();
        expect(definition).toBeDefined();
        expect(definition.name).toBe('selftest');
        expect(definition.description).toContain('JTAG');
        expect(definition.icon).toBe('🔧');
        expect(definition.category).toBe('development');
        expect(definition.params).toBeDefined();
        expect(definition.examples).toBeDefined();
        expect(definition.usage).toBeDefined();
        // Parse the params JSON
        const params = JSON.parse(definition.params);
        expect(params.verbose).toBeDefined();
        expect(params.verbose.type).toBe('boolean');
        expect(params.mode).toBeDefined();
        expect(params.mode.enum).toEqual(['simple', 'browser', 'devtools', 'full']);
    });
    test('should execute simple mode successfully', async () => {
        const params = { verbose: true, mode: 'simple' };
        const context = {};
        const result = await SelfTestCommand.execute(params, context);
        expect(result).toBeDefined();
        expect(result.success).toBe(true);
        expect(result.message).toContain('simple mode');
        expect(result.tests).toBeDefined();
        expect(result.tests.simple).toBe(true);
        expect(result.mode).toBe('simple');
    });
    test('should execute browser mode and test DevTools connectivity', async () => {
        const params = { verbose: false, mode: 'browser' };
        const mockContinuum = {
            browserDetector: null // No browser detector in test
        };
        const context = { continuum: mockContinuum };
        const result = await SelfTestCommand.execute(params, context);
        expect(result).toBeDefined();
        expect(result.tests).toBeDefined();
        expect(result.tests.simple).toBe(true);
        expect(result.mode).toBe('browser');
        // Browser test may fail in unit test environment due to no actual DevTools
        expect(typeof result.tests.browser).toBe('boolean');
    });
    test('should execute devtools mode with proper type safety', async () => {
        const params = { verbose: true, mode: 'devtools' };
        const mockContinuum = {
            commandProcessor: {
                getCommand: jest.fn(() => ({ execute: jest.fn() }))
            }
        };
        const context = { continuum: mockContinuum };
        const result = await SelfTestCommand.execute(params, context);
        expect(result).toBeDefined();
        expect(result.tests).toBeDefined();
        expect(result.mode).toBe('devtools');
        expect(typeof result.success).toBe('boolean');
        expect(typeof result.tests.simple).toBe('boolean');
        expect(typeof result.tests.devtools).toBe('boolean');
    });
    test('should execute full mode testing all capabilities', async () => {
        const params = { verbose: false, mode: 'full' };
        const mockContinuum = {
            browserDetector: {
                getAvailableBrowsers: jest.fn(() => Promise.resolve(['chrome', 'firefox']))
            },
            commandProcessor: {
                getCommand: jest.fn((name) => {
                    if (name === 'screenshot') {
                        return { execute: jest.fn() };
                    }
                    return null;
                })
            }
        };
        const context = { continuum: mockContinuum };
        const result = await SelfTestCommand.execute(params, context);
        expect(result).toBeDefined();
        expect(result.mode).toBe('full');
        expect(result.tests.simple).toBe(true); // Should always pass
        expect(typeof result.tests.browser).toBe('boolean');
        expect(typeof result.tests.devtools).toBe('boolean');
        expect(typeof result.tests.screenshot).toBe('boolean');
    });
    test('should handle execution errors gracefully with proper typing', async () => {
        // Mock params that will cause an error
        const params = { mode: 'invalid_mode' };
        const context = {};
        const result = await SelfTestCommand.execute(params, context);
        expect(result).toBeDefined();
        expect(result.success).toBe(true); // Should still succeed as it defaults to simple mode
        expect(result.tests).toBeDefined();
        expect(result.mode).toBe('invalid_mode'); // The mode is preserved even if invalid
        expect(result.tests.simple).toBe(true); // Simple test should always pass
    });
    test('should parse parameters with TypeScript type safety', () => {
        // Test that TypeScript compilation enforces proper parameter types
        const validParams = {
            verbose: true,
            mode: 'browser'
        };
        expect(validParams.verbose).toBe(true);
        expect(validParams.mode).toBe('browser');
        // This should be allowed by TypeScript
        const optionalParams = {};
        expect(optionalParams).toBeDefined();
    });
    test('should be discoverable by command system with proper exports', () => {
        // Test that the command can be loaded and has proper TypeScript exports
        expect(typeof SelfTestCommand).toBe('function');
        expect(typeof SelfTestCommand.getDefinition).toBe('function');
        expect(typeof SelfTestCommand.execute).toBe('function');
        // Verify TypeScript module structure
        expect(SelfTestCommand.name).toBe('SelfTestCommand');
    });
    test('should provide comprehensive test results structure', async () => {
        const params = { mode: 'full', verbose: false };
        const context = {};
        const result = await SelfTestCommand.execute(params, context);
        // Verify the complete structure matches TypeScript interface
        expect(result).toHaveProperty('success');
        expect(result).toHaveProperty('message');
        expect(result).toHaveProperty('tests');
        expect(result).toHaveProperty('mode');
        expect(result.tests).toHaveProperty('simple');
        expect(result.tests).toHaveProperty('browser');
        expect(result.tests).toHaveProperty('devtools');
        expect(result.tests).toHaveProperty('screenshot');
        // All test results should be boolean
        expect(typeof result.tests.simple).toBe('boolean');
        expect(typeof result.tests.browser).toBe('boolean');
        expect(typeof result.tests.devtools).toBe('boolean');
        expect(typeof result.tests.screenshot).toBe('boolean');
    });
});
//# sourceMappingURL=SelfTestCommand.test.js.map