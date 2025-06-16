/**
 * Unit Tests for JavaScript Validation
 * Test that continuum rejects garbage JS code before execution
 * Includes syntax validation, security checks, and AI sheriff monitoring
 */

describe('JavaScript Validation and Security', () => {
    let mockContinuum;
    let mockJSValidator;
    let mockAISheriff;
    
    beforeEach(() => {
        // Mock console
        global.console = {
            log: jest.fn(),
            warn: jest.fn(),
            error: jest.fn()
        };
        
        // Mock JavaScript Validator
        mockJSValidator = {
            validateSyntax: function(code) {
                try {
                    new Function(code);
                    return { valid: true, errors: [] };
                } catch (error) {
                    return { 
                        valid: false, 
                        errors: [error.message],
                        type: 'syntax_error'
                    };
                }
            },
            
            validateSecurity: function(code) {
                const dangerousPatterns = [
                    /eval\s*\(/,
                    /Function\s*\(/,
                    /document\.write/,
                    /window\.location/,
                    /localStorage\.clear/,
                    /sessionStorage\.clear/,
                    /document\.cookie\s*=/,
                    /XMLHttpRequest/,
                    /fetch\s*\(/,
                    /\.innerHTML\s*=/,
                    /alert\s*\(/,
                    /confirm\s*\(/,
                    /prompt\s*\(/
                ];
                
                const violations = [];
                dangerousPatterns.forEach((pattern, index) => {
                    if (pattern.test(code)) {
                        violations.push({
                            pattern: pattern.source,
                            risk: index < 3 ? 'high' : index < 8 ? 'medium' : 'low',
                            message: `Potentially dangerous code detected: ${pattern.source}`
                        });
                    }
                });
                
                return {
                    safe: violations.length === 0,
                    violations: violations,
                    riskLevel: violations.length > 0 ? Math.max(...violations.map(v => 
                        v.risk === 'high' ? 3 : v.risk === 'medium' ? 2 : 1
                    )) : 0
                };
            },
            
            validateComplexity: function(code) {
                const lines = code.split('\n').length;
                const characters = code.length;
                const functions = (code.match(/function\s+\w+/g) || []).length;
                const variables = (code.match(/(?:var|let|const)\s+\w+/g) || []).length;
                
                return {
                    metrics: { lines, characters, functions, variables },
                    complexity: lines > 100 || characters > 5000 ? 'high' : 
                               lines > 50 || characters > 2000 ? 'medium' : 'low',
                    recommendation: lines > 100 ? 'Consider breaking into smaller functions' : 'Acceptable complexity'
                };
            }
        };
        
        // Mock AI Sheriff
        mockAISheriff = {
            reviewCode: function(code, context = {}) {
                const issues = [];
                
                // Check for common anti-patterns
                if (code.includes('document.body')) {
                    issues.push({
                        type: 'anti-pattern',
                        severity: 'medium',
                        message: 'Direct document.body manipulation detected. Consider using more specific selectors.',
                        suggestion: 'Use querySelector("#main-content") or similar specific selectors'
                    });
                }
                
                if (code.includes('setTimeout') && !code.includes('clearTimeout')) {
                    issues.push({
                        type: 'resource-leak',
                        severity: 'low',
                        message: 'setTimeout without clearTimeout may cause memory leaks',
                        suggestion: 'Store timeout ID and clear when done'
                    });
                }
                
                if (code.includes('console.log') && context.production) {
                    issues.push({
                        type: 'production-warning',
                        severity: 'low',
                        message: 'console.log statements in production code',
                        suggestion: 'Remove debug statements or use proper logging'
                    });
                }
                
                // Check for best practices
                const hasErrorHandling = code.includes('try') || code.includes('catch') || code.includes('.catch(');
                if (code.includes('Promise') && !hasErrorHandling) {
                    issues.push({
                        type: 'missing-error-handling',
                        severity: 'medium',
                        message: 'Promise usage without error handling',
                        suggestion: 'Add .catch() or try/catch blocks'
                    });
                }
                
                return {
                    approved: issues.filter(i => i.severity === 'high').length === 0,
                    issues: issues,
                    score: Math.max(0, 100 - (issues.length * 10)),
                    recommendation: issues.length === 0 ? 'Code approved' : 'Review suggested improvements'
                };
            }
        };
        
        // Mock Continuum with validation pipeline
        mockContinuum = {
            validateAndExecute: async function(code, options = {}) {
                // Step 1: Syntax validation
                const syntaxResult = mockJSValidator.validateSyntax(code);
                if (!syntaxResult.valid) {
                    return Promise.reject(new Error(`Syntax Error: ${syntaxResult.errors[0]}`));
                }
                
                // Step 2: Security validation
                const securityResult = mockJSValidator.validateSecurity(code);
                if (!securityResult.safe && securityResult.riskLevel >= 3) {
                    return Promise.reject(new Error(`Security Risk: ${securityResult.violations[0].message}`));
                }
                
                // Step 3: AI Sheriff review
                const sheriffReview = mockAISheriff.reviewCode(code, options);
                if (!sheriffReview.approved) {
                    const highSeverityIssues = sheriffReview.issues.filter(i => i.severity === 'high');
                    if (highSeverityIssues.length > 0) {
                        return Promise.reject(new Error(`AI Sheriff Rejection: ${highSeverityIssues[0].message}`));
                    }
                }
                
                // Step 4: Execute if all validations pass
                try {
                    const result = eval(`(function() { ${code} })()`);
                    return Promise.resolve({
                        success: true,
                        result: result,
                        validationScore: sheriffReview.score,
                        warnings: sheriffReview.issues.filter(i => i.severity !== 'high')
                    });
                } catch (error) {
                    return Promise.reject(new Error(`Execution Error: ${error.message}`));
                }
            }
        };
    });
    
    describe('Syntax Validation', () => {
        test('rejects code with syntax errors', async () => {
            const garbageCode = `
                var x = 10
                if (x > 5 {  // Missing closing parenthesis
                    console.log("test"
                }
            `;
            
            expect.assertions(2);
            
            try {
                await mockContinuum.validateAndExecute(garbageCode);
                fail('Should have rejected garbage code');
            } catch (error) {
                expect(error).toBeInstanceOf(Error);
                expect(error.message).toContain('Syntax Error');
            }
        });
        
        test('rejects incomplete function definitions', async () => {
            const incompleteCode = `
                function badFunction() {
                    return "test"
                // Missing closing brace
            `;
            
            expect.assertions(1);
            
            try {
                await mockContinuum.validateAndExecute(incompleteCode);
                fail('Should have rejected incomplete code');
            } catch (error) {
                expect(error.message).toContain('Syntax Error');
            }
        });
        
        test('accepts valid JavaScript code', async () => {
            const validCode = `
                var x = 10;
                if (x > 5) {
                    return "valid code";
                }
            `;
            
            const result = await mockContinuum.validateAndExecute(validCode);
            
            expect(result.success).toBe(true);
            expect(result.result).toBe("valid code");
        });
    });
    
    describe('Security Validation', () => {
        test('rejects high-risk eval() usage', async () => {
            const dangerousCode = `
                var userInput = "alert('hacked')";
                eval(userInput);
            `;
            
            expect.assertions(2);
            
            try {
                await mockContinuum.validateAndExecute(dangerousCode);
                fail('Should have rejected dangerous code');
            } catch (error) {
                expect(error).toBeInstanceOf(Error);
                expect(error.message).toContain('Security Risk');
            }
        });
        
        test('rejects document.write injections', async () => {
            const injectionCode = `
                document.write('<script src="malicious.js"></script>');
            `;
            
            expect.assertions(1);
            
            try {
                await mockContinuum.validateAndExecute(injectionCode);
                fail('Should have rejected injection code');
            } catch (error) {
                expect(error.message).toContain('Security Risk');
            }
        });
        
        test('allows safe DOM operations', async () => {
            const safeCode = `
                var element = document.querySelector('#safe-element');
                if (element) {
                    element.textContent = 'Safe update';
                }
                return 'safe operation';
            `;
            
            const result = await mockContinuum.validateAndExecute(safeCode);
            
            expect(result.success).toBe(true);
            expect(result.warnings).toBeDefined();
        });
    });
    
    describe('AI Sheriff Code Review', () => {
        test('warns about document.body anti-patterns', async () => {
            const antiPatternCode = `
                var element = document.body;
                element.style.background = 'red';
                return 'modified body';
            `;
            
            const result = await mockContinuum.validateAndExecute(antiPatternCode);
            
            expect(result.success).toBe(true);
            expect(result.warnings).toBeDefined();
            expect(result.warnings.some(w => w.type === 'anti-pattern')).toBe(true);
            expect(result.warnings.some(w => w.message.includes('document.body'))).toBe(true);
        });
        
        test('warns about missing error handling', async () => {
            const noErrorHandlingCode = `
                var promise = new Promise((resolve) => {
                    resolve('test');
                });
                return promise;
            `;
            
            const result = await mockContinuum.validateAndExecute(noErrorHandlingCode);
            
            expect(result.success).toBe(true);
            expect(result.warnings.some(w => w.type === 'missing-error-handling')).toBe(true);
        });
        
        test('provides improvement suggestions', async () => {
            const improvableCode = `
                setTimeout(function() {
                    console.log('delayed action');
                }, 1000);
                return 'timer set';
            `;
            
            const result = await mockContinuum.validateAndExecute(improvableCode);
            
            expect(result.success).toBe(true);
            expect(result.warnings.some(w => w.suggestion.includes('clearTimeout'))).toBe(true);
        });
        
        test('rejects code with high severity issues', async () => {
            // Add a high severity issue to the sheriff
            const originalReview = mockAISheriff.reviewCode;
            mockAISheriff.reviewCode = function(code) {
                const baseResult = originalReview.call(this, code);
                baseResult.issues.push({
                    type: 'critical-security',
                    severity: 'high',
                    message: 'Critical security vulnerability detected',
                    suggestion: 'Rewrite using secure patterns'
                });
                baseResult.approved = false;
                return baseResult;
            };
            
            const suspiciousCode = `
                var data = getUserInput();
                return data;
            `;
            
            expect.assertions(2);
            
            try {
                await mockContinuum.validateAndExecute(suspiciousCode);
                fail('Should have rejected high severity code');
            } catch (error) {
                expect(error).toBeInstanceOf(Error);
                expect(error.message).toContain('AI Sheriff Rejection');
            }
        });
    });
    
    describe('Multi-Level Validation Pipeline', () => {
        test('processes all validation stages in order', async () => {
            const validCode = `
                var x = Math.random();
                return x > 0.5 ? 'high' : 'low';
            `;
            
            const result = await mockContinuum.validateAndExecute(validCode);
            
            expect(result.success).toBe(true);
            expect(result.validationScore).toBeGreaterThan(0);
            expect(result.warnings).toBeDefined();
        });
        
        test('stops at first critical failure', async () => {
            const syntaxErrorCode = `
                var x = 10
                eval("bad code");  // Would be security issue, but syntax error comes first
                if (x > 5 {  // Syntax error
                    return "test";
                }
            `;
            
            expect.assertions(1);
            
            try {
                await mockContinuum.validateAndExecute(syntaxErrorCode);
                fail('Should have rejected at syntax stage');
            } catch (error) {
                expect(error.message).toContain('Syntax Error');
            }
        });
        
        test('accumulates warnings from all stages', async () => {
            const warningCode = `
                var element = document.body;  // Anti-pattern warning
                setTimeout(function() {       // Resource leak warning
                    console.log('test');      // Production warning (if production context)
                }, 1000);
                return 'warnings test';
            `;
            
            const result = await mockContinuum.validateAndExecute(warningCode, { production: true });
            
            expect(result.success).toBe(true);
            expect(result.warnings.length).toBeGreaterThan(1);
            
            const warningTypes = result.warnings.map(w => w.type);
            expect(warningTypes).toContain('anti-pattern');
            expect(warningTypes).toContain('resource-leak');
            expect(warningTypes).toContain('production-warning');
        });
    });
    
    describe('Continuum-Specific Validation', () => {
        test('validates screenshot command code', async () => {
            const screenshotCode = `
                if (typeof window.ScreenshotUtils === 'undefined') {
                    throw new Error('ScreenshotUtils not available');
                }
                
                return window.ScreenshotUtils.takeScreenshot(document.body, {
                    scale: 0.5,
                    source: 'validation_test'
                });
            `;
            
            const result = await mockContinuum.validateAndExecute(screenshotCode);
            
            expect(result.success).toBe(true);
            expect(result.warnings.some(w => w.message.includes('document.body'))).toBe(true);
        });
        
        test('rejects malicious continuum API usage', async () => {
            const maliciousCode = `
                window.continuum = null;  // Attempt to break continuum
                eval("window.location = 'http://evil.com'");
            `;
            
            expect.assertions(1);
            
            try {
                await mockContinuum.validateAndExecute(maliciousCode);
                fail('Should have rejected malicious code');
            } catch (error) {
                expect(error.message).toContain('Security Risk');
            }
        });
        
        test('validates agent registration code', async () => {
            const agentCode = `
                return {
                    agentId: 'test-agent',
                    agentName: 'Test Agent',
                    agentType: 'ai',
                    capabilities: ['screenshot', 'validation']
                };
            `;
            
            const result = await mockContinuum.validateAndExecute(agentCode);
            
            expect(result.success).toBe(true);
            expect(result.result.agentId).toBe('test-agent');
            expect(result.validationScore).toBeGreaterThan(90); // High score for clean code
        });
    });
});

// Export validation utilities for other tests
const jsValidationUtils = {
    createGarbageCode: (type = 'syntax') => {
        const garbageTypes = {
            syntax: 'var x = 10; if (x > 5 { console.log("missing paren"); }',
            security: 'eval("alert(\'hacked\')"); document.write("<script>evil()</script>");',
            antipattern: 'document.body.innerHTML = userInput; setTimeout(() => {}, 1000);',
            incomplete: 'function test() { return "incomplete'
        };
        return garbageTypes[type] || garbageTypes.syntax;
    },
    
    createValidCode: (purpose = 'general') => {
        const validCodes = {
            general: 'var x = 10; return x > 5 ? "high" : "low";',
            screenshot: 'return window.ScreenshotUtils ? "available" : "not available";',
            agent: 'return { agentId: "test", status: "ready" };',
            dom: 'var el = document.querySelector("#test"); return el ? "found" : "not found";'
        };
        return validCodes[purpose] || validCodes.general;
    },
    
    expectValidationRejection: async (validateFunction, code, expectedErrorType) => {
        let error = null;
        try {
            await validateFunction(code);
            throw new Error('Validation should have rejected the code');
        } catch (e) {
            error = e;
        }
        
        expect(error).not.toBeNull();
        expect(error.message.toLowerCase()).toContain(expectedErrorType.toLowerCase());
        return error;
    }
};

module.exports = { jsValidationUtils };