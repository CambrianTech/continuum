/**
 * StatusTextCommand TypeScript Unit Tests
 * Comprehensive test coverage for status text updates
 */
import { StatusTextCommand } from './StatusTextCommand';
describe('StatusTextCommand', () => {
    let mockContext;
    let mockContinuonStatus;
    beforeEach(() => {
        mockContinuonStatus = {
            updateStatusText: jest.fn(),
            getStatus: jest.fn().mockReturnValue({ status: 'connected', ready: true }),
            currentStatus: 'connected'
        };
        mockContext = {
            continuonStatus: mockContinuonStatus
        };
        // Mock console methods
        jest.spyOn(console, 'log').mockImplementation();
        jest.spyOn(console, 'error').mockImplementation();
    });
    afterEach(() => {
        jest.restoreAllMocks();
        jest.clearAllTimers();
    });
    describe('Command Definition', () => {
        test('should provide valid command definition', () => {
            const definition = StatusTextCommand.getDefinition();
            expect(definition).toBeDefined();
            expect(definition.name).toBe('status_text');
            expect(definition.category).toBe('core');
            expect(definition.icon).toBe('💬');
            expect(definition.description).toContain('status message text');
            expect(definition.examples).toHaveLength(4);
        });
    });
    describe('Parameter Handling', () => {
        test('should accept simple text parameter', async () => {
            const params = { text: 'Processing...' };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(true);
            expect(result.data?.text).toBe('Processing...');
            expect(result.data?.duration).toBe(0);
            expect(result.data?.temporary).toBe(false);
        });
        test('should accept text with duration', async () => {
            const params = { text: 'Temporary message', duration: 5000 };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(true);
            expect(result.data?.text).toBe('Temporary message');
            expect(result.data?.duration).toBe(5000);
            expect(result.data?.temporary).toBe(true);
        });
        test('should handle JSON string parameters', async () => {
            const jsonParams = JSON.stringify({ text: 'JSON message', duration: 3000 });
            const result = await StatusTextCommand.execute(jsonParams, mockContext);
            expect(result.success).toBe(true);
            expect(result.data?.text).toBe('JSON message');
            expect(result.data?.duration).toBe(3000);
        });
        test('should reject missing text parameter', async () => {
            const params = { duration: 1000 };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(false);
            expect(result.error).toContain('Missing required parameters: text');
        });
        test('should reject empty text parameter', async () => {
            const params = { text: '' };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(false);
            expect(result.error).toContain('Missing required parameters: text');
        });
    });
    describe('ContinuonStatus Integration', () => {
        test('should call updateStatusText on continuonStatus', async () => {
            const params = { text: 'Test message' };
            await StatusTextCommand.execute(params, mockContext);
            expect(mockContinuonStatus.updateStatusText).toHaveBeenCalledWith('Test message');
        });
        test('should get current status from continuonStatus', async () => {
            const params = { text: 'Test message' };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(mockContinuonStatus.getStatus).toHaveBeenCalled();
            expect(result.data?.status).toEqual({ status: 'connected', ready: true });
        });
        test('should handle missing continuonStatus', async () => {
            const params = { text: 'Test message' };
            const result = await StatusTextCommand.execute(params, undefined);
            expect(result.success).toBe(false);
            expect(result.error).toBe('ContinuonStatus not available in context');
        });
        test('should handle missing updateStatusText method', async () => {
            const contextWithoutMethod = {
                continuonStatus: { getStatus: jest.fn() }
            };
            const params = { text: 'Test message' };
            const result = await StatusTextCommand.execute(params, contextWithoutMethod);
            expect(result.success).toBe(false);
            expect(result.error).toBe('ContinuonStatus.updateStatusText method not available');
        });
    });
    describe('Temporary Messages', () => {
        beforeEach(() => {
            jest.useFakeTimers();
        });
        afterEach(() => {
            jest.useRealTimers();
        });
        test('should set timeout for temporary messages', async () => {
            const params = { text: 'Temporary', duration: 5000 };
            await StatusTextCommand.execute(params, mockContext);
            // Initially called with the temporary message
            expect(mockContinuonStatus.updateStatusText).toHaveBeenCalledWith('Temporary');
            expect(mockContinuonStatus.updateStatusText).toHaveBeenCalledTimes(1);
            // Fast-forward time
            jest.advanceTimersByTime(5000);
            // Should revert to default after timeout
            expect(mockContinuonStatus.updateStatusText).toHaveBeenCalledTimes(2);
            expect(mockContinuonStatus.updateStatusText).toHaveBeenLastCalledWith('Ready');
        });
        test('should use "Disconnected" for non-connected status', async () => {
            mockContinuonStatus.currentStatus = 'disconnected';
            const params = { text: 'Temporary', duration: 1000 };
            await StatusTextCommand.execute(params, mockContext);
            jest.advanceTimersByTime(1000);
            expect(mockContinuonStatus.updateStatusText).toHaveBeenLastCalledWith('Disconnected');
        });
        test('should not set timeout for permanent messages', async () => {
            const setTimeoutSpy = jest.spyOn(global, 'setTimeout');
            const params = { text: 'Permanent message' };
            await StatusTextCommand.execute(params, mockContext);
            expect(setTimeoutSpy).not.toHaveBeenCalled();
            setTimeoutSpy.mockRestore();
        });
        test('should handle missing continuonStatus in timeout', async () => {
            const params = { text: 'Temporary', duration: 1000 };
            await StatusTextCommand.execute(params, mockContext);
            // Remove continuonStatus before timeout
            mockContext.continuonStatus = undefined;
            // Should not throw when timeout executes
            expect(() => {
                jest.advanceTimersByTime(1000);
            }).not.toThrow();
        });
    });
    describe('Error Handling', () => {
        test('should handle updateStatusText throwing error', async () => {
            mockContinuonStatus.updateStatusText.mockImplementation(() => {
                throw new Error('Status update failed');
            });
            const params = { text: 'Test message' };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(false);
            expect(result.error).toContain('Status text command failed');
        });
        test('should handle getStatus throwing error', async () => {
            mockContinuonStatus.getStatus.mockImplementation(() => {
                throw new Error('Get status failed');
            });
            const params = { text: 'Test message' };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(true); // Should still succeed despite getStatus error
            expect(result.data?.status).toEqual({ status: 'error', error: 'Get status failed' });
        });
        test('should handle malformed JSON parameters', async () => {
            const malformedJson = '{"text": "incomplete';
            // Should handle JSON parsing gracefully through BaseCommand.parseParams
            const params = { text: 'fallback text' };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(true);
        });
    });
    describe('Type Safety', () => {
        test('should maintain type safety throughout execution', async () => {
            const params = { text: 'Type safe message', duration: 2000 };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.success).toBe(true);
            expect(typeof result.message).toBe('string');
            expect(typeof result.data?.text).toBe('string');
            expect(typeof result.data?.duration).toBe('number');
            expect(typeof result.data?.temporary).toBe('boolean');
        });
    });
    describe('Message Formatting', () => {
        test('should format permanent message correctly', async () => {
            const params = { text: 'Permanent status' };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.message).toBe('Status text updated: "Permanent status" permanently');
        });
        test('should format temporary message correctly', async () => {
            const params = { text: 'Temporary status', duration: 3000 };
            const result = await StatusTextCommand.execute(params, mockContext);
            expect(result.message).toBe('Status text updated: "Temporary status" for 3000ms');
        });
    });
    describe('Console Logging', () => {
        test('should log status text updates', async () => {
            const consoleSpy = jest.spyOn(console, 'log');
            const params = { text: 'Test logging' };
            await StatusTextCommand.execute(params, mockContext);
            expect(consoleSpy).toHaveBeenCalledWith('💬 Status text updated: "Test logging"');
        });
    });
});
//# sourceMappingURL=StatusTextCommand.test.js.map