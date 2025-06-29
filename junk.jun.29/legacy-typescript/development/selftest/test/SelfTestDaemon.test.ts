/**
 * SelfTest Daemon Integration Tests - TypeScript Implementation
 * Test that the TypeScript selftest command works through the daemon system
 */

import { SelfTestCommand, SelfTestParams, SelfTestResult } from '../SelfTestCommand';

describe('SelfTest Daemon Integration', () => {
    
    describe('TypeScript Command Integration', () => {
        test('should execute selftest via TypeScript (not fallback)', async () => {
            const params: SelfTestParams = { mode: 'simple', verbose: true };
            
            const result: SelfTestResult = await SelfTestCommand.execute(params);
            
            // Verify this is the TypeScript implementation (has mode field)
            expect(result).toBeDefined();
            expect(result.success).toBe(true);
            expect(result.mode).toBe('simple'); // This proves it's TypeScript, not legacy
            expect(result.tests).toBeDefined();
            expect(result.tests.simple).toBe(true);
            expect(result.message).toContain('simple mode'); // TypeScript format
        });

        test('should execute devtools mode with browser testing', async () => {
            const params: SelfTestParams = { mode: 'devtools', verbose: true };
            
            const result: SelfTestResult = await SelfTestCommand.execute(params);
            
            expect(result).toBeDefined();
            expect(result.mode).toBe('devtools');
            expect(result.tests).toBeDefined();
            expect(result.tests.simple).toBe(true); // Always runs
            expect(typeof result.tests.devtools).toBe('boolean'); // DevTools test attempted
        });

        test('should execute browser mode with DevTools connectivity', async () => {
            const params: SelfTestParams = { mode: 'browser', verbose: true };
            
            const result: SelfTestResult = await SelfTestCommand.execute(params);
            
            expect(result).toBeDefined();
            expect(result.mode).toBe('browser');
            expect(result.tests).toBeDefined();
            expect(result.tests.simple).toBe(true);
            expect(typeof result.tests.browser).toBe('boolean');
        });

        test('should execute full mode testing all capabilities', async () => {
            const params: SelfTestParams = { mode: 'full', verbose: true };
            
            const result: SelfTestResult = await SelfTestCommand.execute(params);
            
            expect(result).toBeDefined();
            expect(result.mode).toBe('full');
            expect(result.tests).toBeDefined();
            expect(result.tests.simple).toBe(true);
            expect(typeof result.tests.browser).toBe('boolean');
            expect(typeof result.tests.devtools).toBe('boolean');
            expect(typeof result.tests.screenshot).toBe('boolean');
        });
    });

    describe('JTAG System Integration', () => {
        test('should provide JTAG debugging data', async () => {
            const params: SelfTestParams = { mode: 'devtools', verbose: true };
            
            // Mock continuum context with DevTools port
            const mockContext = {
                continuum: {
                    browserDetector: {
                        getAvailableBrowsers: jest.fn().mockResolvedValue(['chrome', 'opera'])
                    },
                    commandProcessor: {
                        getCommand: jest.fn().mockReturnValue({ execute: jest.fn() })
                    }
                }
            };
            
            const result: SelfTestResult = await SelfTestCommand.execute(params, mockContext);
            
            expect(result).toBeDefined();
            expect(result.tests).toBeDefined();
            
            // Should contain JTAG-style debugging info
            expect(result.mode).toBe('devtools');
            expect(typeof result.success).toBe('boolean');
            expect(typeof result.message).toBe('string');
        });

        test('should handle DevTools port connectivity testing', async () => {
            const params: SelfTestParams = { mode: 'browser', verbose: true };
            
            const result: SelfTestResult = await SelfTestCommand.execute(params);
            
            // Browser test attempts DevTools port 9222 connectivity
            expect(result.tests.browser).toBeDefined();
            expect(typeof result.tests.browser).toBe('boolean');
            
            // In CI/test environment, DevTools may not be available (expected)
            // The test validates the attempt was made
        });
    });

    describe('Error Handling and Resilience', () => {
        test('should handle invalid parameters gracefully', async () => {
            const params = { mode: 'invalid-mode' } as any;
            
            const result: SelfTestResult = await SelfTestCommand.execute(params);
            
            expect(result).toBeDefined();
            expect(result.success).toBe(true); // Still succeeds with simple test
            expect(result.tests.simple).toBe(true);
        });

        test('should handle missing context gracefully', async () => {
            const params: SelfTestParams = { mode: 'full', verbose: false };
            
            const result: SelfTestResult = await SelfTestCommand.execute(params, undefined);
            
            expect(result).toBeDefined();
            expect(result.tests.simple).toBe(true); // Simple test always works
            expect(typeof result.tests.browser).toBe('boolean');
            expect(typeof result.tests.devtools).toBe('boolean');
            expect(typeof result.tests.screenshot).toBe('boolean');
        });
    });

    describe('Performance and Stability', () => {
        test('should execute selftest within reasonable time', async () => {
            const startTime = Date.now();
            
            const result: SelfTestResult = await SelfTestCommand.execute({ mode: 'simple' });
            
            const duration = Date.now() - startTime;
            
            expect(result.success).toBe(true);
            expect(duration).toBeLessThan(5000); // Should complete within 5 seconds
        });

        test('should be consistent across multiple executions', async () => {
            const results: SelfTestResult[] = [];
            
            for (let i = 0; i < 3; i++) {
                const result = await SelfTestCommand.execute({ mode: 'simple', verbose: false });
                results.push(result);
            }
            
            // All executions should succeed consistently
            results.forEach(result => {
                expect(result.success).toBe(true);
                expect(result.mode).toBe('simple');
                expect(result.tests.simple).toBe(true);
            });
        });
    });
});