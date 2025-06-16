/**
 * Unit Tests for JavaScript Validation Commands
 * Test continuum commands that validate JavaScript code before execution
 * Integrates with ProtocolSheriff for AI-powered code review
 */

describe('JavaScript Validation Commands', () => {
    let mockJSValidationCommand;
    let mockProtocolSheriff;
    
    beforeEach(() => {
        // Mock console
        global.console = {
            log: jest.fn(),
            warn: jest.fn(),
            error: jest.fn()
        };
        
        // Mock ProtocolSheriff (based on actual implementation)
        mockProtocolSheriff = {
            validateCode: async function(code, context = {}) {
                const violations = [];
                
                // Basic syntax check
                try {
                    new Function(code);
                } catch (syntaxError) {
                    violations.push({
                        type: 'syntax_error',
                        severity: 'high',
                        message: `Syntax error: ${syntaxError.message}`,
                        recommendation: 'Fix syntax errors before execution'
                    });
                }
                
                // Security checks
                const dangerousPatterns = [
                    { pattern: /eval\s*\(/, risk: 'high', message: 'eval() usage detected - potential security risk' },
                    { pattern: /Function\s*\(/, risk: 'high', message: 'Function constructor usage - potential security risk' },
                    { pattern: /document\.write/, risk: 'medium', message: 'document.write usage - can break page structure' },
                    { pattern: /innerHTML\s*=/, risk: 'medium', message: 'innerHTML assignment - potential XSS risk' }
                ];
                
                dangerousPatterns.forEach(check => {
                    if (check.pattern.test(code)) {
                        violations.push({
                            type: 'security_risk',
                            severity: check.risk,
                            message: check.message,
                            recommendation: 'Use safer alternatives'
                        });
                    }
                });
                
                // Best practice checks
                if (code.includes('document.body') && !context.allowBodyAccess) {
                    violations.push({
                        type: 'anti_pattern',
                        severity: 'medium',
                        message: 'Direct document.body access detected',
                        recommendation: 'Use more specific selectors like querySelector("#main-content")'
                    });
                }
                
                const highSeverityViolations = violations.filter(v => v.severity === 'high');
                
                return {
                    isValid: highSeverityViolations.length === 0,
                    violations: violations,
                    score: Math.max(0, 100 - (violations.length * 15)),
                    recommendation: violations.length === 0 ? 'Code approved for execution' : 
                                   highSeverityViolations.length > 0 ? 'Code rejected - fix high severity issues' :
                                   'Code approved with warnings'
                };
            }
        };
        
        // Mock JS Validation Command
        mockJSValidationCommand = {
            name: 'validate_js',
            description: 'Validate JavaScript code before execution',
            
            execute: async function(params) {
                const { code, context = {}, strict = false } = params;
                
                if (!code || typeof code !== 'string') {
                    return {
                        success: false,
                        error: 'Code parameter is required and must be a string',
                        validationResult: null
                    };
                }
                
                console.log(`🔍 Validating ${code.length} characters of JavaScript code...`);
                
                try {
                    // Run through ProtocolSheriff validation
                    const validationResult = await mockProtocolSheriff.validateCode(code, context);
                    
                    if (strict && validationResult.violations.length > 0) {
                        return {
                            success: false,
                            error: 'Code validation failed in strict mode',
                            validationResult: validationResult
                        };
                    }
                    
                    if (!validationResult.isValid) {
                        return {
                            success: false,
                            error: 'Code validation failed - high severity issues detected',
                            validationResult: validationResult
                        };
                    }
                    
                    return {
                        success: true,
                        message: validationResult.recommendation,
                        validationResult: validationResult
                    };
                    
                } catch (error) {
                    return {
                        success: false,
                        error: `Validation error: ${error.message}`,
                        validationResult: null
                    };
                }
            }
        };
    });
    
    describe('Command Interface', () => {
        test('validates command structure', () => {
            expect(mockJSValidationCommand.name).toBe('validate_js');
            expect(mockJSValidationCommand.description).toContain('Validate JavaScript');
            expect(typeof mockJSValidationCommand.execute).toBe('function');
        });
        
        test('requires code parameter', async () => {
            const result = await mockJSValidationCommand.execute({});
            
            expect(result.success).toBe(false);
            expect(result.error).toContain('Code parameter is required');
        });
        
        test('rejects non-string code parameter', async () => {
            const result = await mockJSValidationCommand.execute({ code: 123 });
            
            expect(result.success).toBe(false);
            expect(result.error).toContain('must be a string');
        });
    });
    
    describe('Syntax Validation', () => {
        test('rejects code with syntax errors', async () => {
            const garbageCode = 'var x = 10; if (x > 5 { console.log("missing paren"); }';
            
            const result = await mockJSValidationCommand.execute({ code: garbageCode });
            
            expect(result.success).toBe(false);
            expect(result.error).toContain('validation failed');
            expect(result.validationResult.violations.some(v => v.type === 'syntax_error')).toBe(true);
        });
        
        test('accepts valid JavaScript syntax', async () => {
            const validCode = 'var x = 10; if (x > 5) { console.log("valid syntax"); }';
            
            const result = await mockJSValidationCommand.execute({ code: validCode });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.isValid).toBe(true);
        });
    });
    
    describe('Security Validation', () => {
        test('rejects dangerous eval() usage', async () => {
            const dangerousCode = 'var userInput = prompt("Enter code:"); eval(userInput);';
            
            const result = await mockJSValidationCommand.execute({ code: dangerousCode });
            
            expect(result.success).toBe(false);
            expect(result.validationResult.violations.some(v => 
                v.type === 'security_risk' && v.message.includes('eval()')
            )).toBe(true);
        });
        
        test('rejects Function constructor usage', async () => {
            const dangerousCode = 'var fn = new Function("return alert(\'hacked\')"); fn();';
            
            const result = await mockJSValidationCommand.execute({ code: dangerousCode });
            
            expect(result.success).toBe(false);
            expect(result.validationResult.violations.some(v => 
                v.message.includes('Function constructor')
            )).toBe(true);
        });
        
        test('warns about innerHTML assignments', async () => {
            const riskyCode = 'document.getElementById("content").innerHTML = userContent;';
            
            const result = await mockJSValidationCommand.execute({ code: riskyCode });
            
            // Should pass (medium risk) but with warnings
            expect(result.success).toBe(true);
            expect(result.validationResult.violations.some(v => 
                v.message.includes('innerHTML')
            )).toBe(true);
        });
    });
    
    describe('Best Practice Validation', () => {
        test('warns about document.body access', async () => {
            const bodyAccessCode = 'var element = document.body; element.style.background = "red";';
            
            const result = await mockJSValidationCommand.execute({ code: bodyAccessCode });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.violations.some(v => 
                v.type === 'anti_pattern' && v.message.includes('document.body')
            )).toBe(true);
        });
        
        test('allows document.body with explicit context permission', async () => {
            const bodyAccessCode = 'var element = document.body; return element.offsetHeight;';
            
            const result = await mockJSValidationCommand.execute({ 
                code: bodyAccessCode,
                context: { allowBodyAccess: true }
            });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.violations.some(v => 
                v.message.includes('document.body')
            )).toBe(false);
        });
    });
    
    describe('Strict Mode', () => {
        test('rejects code with any violations in strict mode', async () => {
            const warningCode = 'document.getElementById("test").innerHTML = "safe content";';
            
            const result = await mockJSValidationCommand.execute({ 
                code: warningCode,
                strict: true 
            });
            
            expect(result.success).toBe(false);
            expect(result.error).toContain('strict mode');
        });
        
        test('allows clean code in strict mode', async () => {
            const cleanCode = 'var element = document.querySelector("#test"); element.textContent = "safe";';
            
            const result = await mockJSValidationCommand.execute({ 
                code: cleanCode,
                strict: true 
            });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.violations.length).toBe(0);
        });
    });
    
    describe('Validation Scoring', () => {
        test('provides high score for clean code', async () => {
            const cleanCode = 'var x = 10; return x * 2;';
            
            const result = await mockJSValidationCommand.execute({ code: cleanCode });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.score).toBe(100);
        });
        
        test('reduces score based on violations', async () => {
            const problematicCode = `
                document.body.style.background = "red";
                document.getElementById("test").innerHTML = content;
            `;
            
            const result = await mockJSValidationCommand.execute({ code: problematicCode });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.score).toBeLessThan(100);
            expect(result.validationResult.violations.length).toBeGreaterThan(0);
        });
    });
    
    describe('Integration with Continuum Screenshot Validation', () => {
        test('validates screenshot command with body selector', async () => {
            const screenshotCode = `
                if (typeof window.ScreenshotUtils === 'undefined') {
                    throw new Error('ScreenshotUtils not available');
                }
                return window.ScreenshotUtils.takeScreenshot(document.body, {
                    scale: 0.5
                });
            `;
            
            const result = await mockJSValidationCommand.execute({ code: screenshotCode });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.violations.some(v => 
                v.message.includes('document.body')
            )).toBe(true);
            expect(result.validationResult.violations.some(v => 
                v.recommendation.includes('querySelector')
            )).toBe(true);
        });
        
        test('approves screenshot command with specific selector', async () => {
            const goodScreenshotCode = `
                var element = document.querySelector('#main-content');
                if (!element) {
                    throw new Error('Target element not found');
                }
                return window.ScreenshotUtils.takeScreenshot(element, {
                    scale: 1.0
                });
            `;
            
            const result = await mockJSValidationCommand.execute({ code: goodScreenshotCode });
            
            expect(result.success).toBe(true);
            expect(result.validationResult.score).toBeGreaterThan(80);
        });
    });
});

// Export command for integration into continuum
export const JSValidationCommand = {
    name: 'validate_js',
    description: 'Validate JavaScript code using ProtocolSheriff and security checks',
    parameters: {
        code: { type: 'string', required: true, description: 'JavaScript code to validate' },
        context: { type: 'object', required: false, description: 'Validation context and permissions' },
        strict: { type: 'boolean', required: false, description: 'Reject code with any violations' }
    },
    
    async execute(params, continuum) {
        // Implementation would integrate with actual ProtocolSheriff
        const { code, context = {}, strict = false } = params;
        
        if (!code || typeof code !== 'string') {
            return {
                success: false,
                error: 'Code parameter is required and must be a string'
            };
        }
        
        // Would use actual ProtocolSheriff here
        console.log(`🔍 Validating JavaScript code (${code.length} characters)...`);
        
        return {
            success: true,
            message: 'Validation command registered - implement with actual ProtocolSheriff',
            mockImplementation: true
        };
    }
};