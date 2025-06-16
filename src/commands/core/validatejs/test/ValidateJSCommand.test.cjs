/**
 * Unit Tests for ValidateJS Command
 * Tests the JavaScript validation command with ProtocolSheriff integration
 */

const ValidateJSCommand = require('../ValidateJSCommand.cjs');

describe('ValidateJSCommand', () => {
    let mockContinuum;
    
    beforeEach(() => {
        // Mock continuum context
        mockContinuum = {
            protocolSheriff: {
                validateCode: jest.fn()
            }
        };
    });
    
    describe('Command Definition', () => {
        test('should have correct command definition', () => {
            const definition = ValidateJSCommand.getDefinition();
            
            expect(definition.name).toBe('validate_js');
            expect(definition.description).toContain('Validate JavaScript');
            expect(definition.parameters.code).toBeDefined();
            expect(definition.parameters.code.required).toBe(true);
        });
    });
    
    describe('Parameter Validation', () => {
        test('should require code parameter', async () => {
            const result = await ValidateJSCommand.execute({}, mockContinuum);
            
            expect(result.success).toBe(false);
            expect(result.message).toContain('code parameter is required');
        });
        
        test('should require string code parameter', async () => {
            const result = await ValidateJSCommand.execute({ code: 123 }, mockContinuum);
            
            expect(result.success).toBe(false);
            expect(result.message).toContain('must be a string');
        });
    });
    
    describe('Code Validation Integration', () => {
        test('should call ProtocolSheriff for validation', async () => {
            const testCode = 'var x = 10; console.log(x);';
            
            mockContinuum.protocolSheriff.validateCode.mockResolvedValue({
                isValid: true,
                violations: [],
                score: 100
            });
            
            const result = await ValidateJSCommand.execute({ code: testCode }, mockContinuum);
            
            expect(mockContinuum.protocolSheriff.validateCode).toHaveBeenCalledWith(testCode, {});
            expect(result.success).toBe(true);
        });
        
        test('should handle validation failures', async () => {
            const badCode = 'eval("malicious code")';
            
            mockContinuum.protocolSheriff.validateCode.mockResolvedValue({
                isValid: false,
                violations: [{ type: 'security_risk', message: 'eval() detected' }],
                score: 0
            });
            
            const result = await ValidateJSCommand.execute({ code: badCode }, mockContinuum);
            
            expect(result.success).toBe(false);
            expect(result.data.violations).toBeDefined();
        });
        
        test('should pass context to ProtocolSheriff', async () => {
            const testCode = 'document.body.style.color = "red";';
            const context = { allowBodyAccess: true };
            
            mockContinuum.protocolSheriff.validateCode.mockResolvedValue({
                isValid: true,
                violations: [],
                score: 95
            });
            
            await ValidateJSCommand.execute({ code: testCode, context }, mockContinuum);
            
            expect(mockContinuum.protocolSheriff.validateCode).toHaveBeenCalledWith(testCode, context);
        });
    });
    
    describe('Strict Mode', () => {
        test('should reject code with warnings in strict mode', async () => {
            const warningCode = 'document.body.innerHTML = "test";';
            
            mockContinuum.protocolSheriff.validateCode.mockResolvedValue({
                isValid: true,
                violations: [{ type: 'warning', severity: 'medium' }],
                score: 85
            });
            
            const result = await ValidateJSCommand.execute({ 
                code: warningCode, 
                strict: true 
            }, mockContinuum);
            
            expect(result.success).toBe(false);
            expect(result.message).toContain('strict mode');
        });
        
        test('should allow clean code in strict mode', async () => {
            const cleanCode = 'var element = document.querySelector("#test");';
            
            mockContinuum.protocolSheriff.validateCode.mockResolvedValue({
                isValid: true,
                violations: [],
                score: 100
            });
            
            const result = await ValidateJSCommand.execute({ 
                code: cleanCode, 
                strict: true 
            }, mockContinuum);
            
            expect(result.success).toBe(true);
        });
    });
    
    describe('Error Handling', () => {
        test('should handle ProtocolSheriff errors gracefully', async () => {
            const testCode = 'some code';
            
            mockContinuum.protocolSheriff.validateCode.mockRejectedValue(
                new Error('ProtocolSheriff unavailable')
            );
            
            const result = await ValidateJSCommand.execute({ code: testCode }, mockContinuum);
            
            expect(result.success).toBe(false);
            expect(result.message).toContain('validation failed');
        });
        
        test('should handle missing ProtocolSheriff', async () => {
            const testCode = 'some code';
            const continuumWithoutSheriff = {};
            
            const result = await ValidateJSCommand.execute({ code: testCode }, continuumWithoutSheriff);
            
            expect(result.success).toBe(false);
            expect(result.message).toContain('ProtocolSheriff not available');
        });
    });
    
    describe('Response Format', () => {
        test('should return properly formatted success response', async () => {
            mockContinuum.protocolSheriff.validateCode.mockResolvedValue({
                isValid: true,
                violations: [],
                score: 100,
                recommendation: 'Code approved'
            });
            
            const result = await ValidateJSCommand.execute({ code: 'valid code' }, mockContinuum);
            
            expect(result).toHaveProperty('success');
            expect(result).toHaveProperty('message');
            expect(result).toHaveProperty('data');
            expect(result.data).toHaveProperty('score');
            expect(result.data).toHaveProperty('violations');
        });
        
        test('should return properly formatted error response', async () => {
            mockContinuum.protocolSheriff.validateCode.mockResolvedValue({
                isValid: false,
                violations: [{ type: 'syntax_error' }],
                score: 0
            });
            
            const result = await ValidateJSCommand.execute({ code: 'bad code' }, mockContinuum);
            
            expect(result).toHaveProperty('success', false);
            expect(result).toHaveProperty('message');
            expect(result).toHaveProperty('data');
        });
    });
});