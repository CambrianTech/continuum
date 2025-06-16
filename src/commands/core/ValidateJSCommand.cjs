/**
 * ValidateJS Command - Validate JavaScript code using ProtocolSheriff
 * Integrates with Academy-trained validation personas
 */

const BaseCommand = require('../BaseCommand.cjs');

class ValidateJSCommand extends BaseCommand {
    constructor() {
        super();
        this.commandName = 'validate_js';
        this.description = 'Validate JavaScript code using AI-powered ProtocolSheriff';
        this.parameters = {
            code: {
                type: 'string',
                required: true,
                description: 'JavaScript code to validate'
            },
            context: {
                type: 'object',
                required: false,
                description: 'Validation context and permissions'
            },
            strict: {
                type: 'boolean',
                required: false,
                default: false,
                description: 'Reject code with any violations (strict mode)'
            },
            allowPatterns: {
                type: 'array',
                required: false,
                description: 'Array of patterns to explicitly allow'
            }
        };
    }

    getDefinition() {
        return {
            name: this.commandName,
            description: this.description,
            parameters: this.parameters
        };
    }

    async execute(params, continuum) {
        const { code, context = {}, strict = false, allowPatterns = [] } = params;
        
        console.log(`🔍 ValidateJS: Validating ${code.length} characters of JavaScript...`);
        
        try {
            // Basic parameter validation
            if (!code || typeof code !== 'string') {
                return this.createErrorResult('Code parameter is required and must be a string');
            }
            
            if (code.length > 50000) {
                return this.createErrorResult('Code too large - maximum 50,000 characters allowed');
            }
            
            // Step 1: Quick syntax check
            const syntaxResult = this.validateSyntax(code);
            if (!syntaxResult.valid) {
                return this.createErrorResult(`Syntax Error: ${syntaxResult.error}`, {
                    validationType: 'syntax',
                    violations: [syntaxResult.error]
                });
            }
            
            // Step 2: Security pattern analysis
            const securityResult = this.validateSecurity(code, allowPatterns);
            
            // Step 3: Use ProtocolSheriff for AI-powered validation
            let sheriffResult = null;
            if (continuum && continuum.protocolSheriff) {
                console.log('🤖 Calling ProtocolSheriff for AI validation...');
                try {
                    sheriffResult = await continuum.protocolSheriff.validateCode(code, {
                        agentRole: 'code_validator',
                        context: context,
                        strict: strict,
                        allowedPatterns: allowPatterns
                    });
                } catch (sheriffError) {
                    console.warn('⚠️ ProtocolSheriff unavailable, using basic validation:', sheriffError.message);
                }
            }
            
            // Combine all validation results
            const allViolations = [
                ...securityResult.violations,
                ...(sheriffResult?.violations || [])
            ];
            
            // Determine overall result
            const highSeverityViolations = allViolations.filter(v => v.severity === 'high');
            const hasHighSeverity = highSeverityViolations.length > 0;
            
            const isValid = !hasHighSeverity && (strict ? allViolations.length === 0 : true);
            
            if (!isValid) {
                const primaryIssue = highSeverityViolations[0] || allViolations[0];
                return this.createErrorResult(
                    strict ? 'Code validation failed in strict mode' : `Code validation failed: ${primaryIssue.message}`,
                    {
                        validationType: 'comprehensive',
                        violations: allViolations,
                        sheriffReview: sheriffResult,
                        score: this.calculateScore(allViolations)
                    }
                );
            }
            
            // Success with possible warnings
            const score = this.calculateScore(allViolations);
            return this.createSuccessResult({
                validationType: 'comprehensive',
                score: score,
                violations: allViolations,
                sheriffReview: sheriffResult,
                recommendation: score === 100 ? 'Code approved - no issues found' :
                               score >= 80 ? 'Code approved with minor warnings' :
                               'Code approved but consider improvements'
            }, `JavaScript validation completed (score: ${score}/100)`);
            
        } catch (error) {
            console.error('❌ ValidateJS execution error:', error);
            return this.createErrorResult(`Validation failed: ${error.message}`);
        }
    }
    
    validateSyntax(code) {
        try {
            // Use Function constructor to check syntax without execution
            new Function(code);
            return { valid: true };
        } catch (syntaxError) {
            return { 
                valid: false, 
                error: syntaxError.message.replace(/Function constructor/, 'Code syntax')
            };
        }
    }
    
    validateSecurity(code, allowPatterns = []) {
        const violations = [];
        
        // Define security patterns
        const securityChecks = [
            {
                pattern: /eval\s*\(/g,
                severity: 'high',
                message: 'eval() usage detected - high security risk',
                recommendation: 'Use JSON.parse() for data or refactor to avoid dynamic evaluation'
            },
            {
                pattern: /Function\s*\(/g,
                severity: 'high', 
                message: 'Function constructor usage - potential code injection risk',
                recommendation: 'Use declared functions or arrow functions instead'
            },
            {
                pattern: /document\.write\s*\(/g,
                severity: 'medium',
                message: 'document.write() usage - can break page structure',
                recommendation: 'Use DOM manipulation methods like appendChild()'
            },
            {
                pattern: /\.innerHTML\s*=/g,
                severity: 'medium',
                message: 'innerHTML assignment - potential XSS vulnerability',
                recommendation: 'Use textContent or createElement/appendChild for safety'
            },
            {
                pattern: /window\.location\s*=/g,
                severity: 'medium',
                message: 'Direct window.location assignment - potential redirect attack',
                recommendation: 'Validate URLs before redirecting'
            },
            {
                pattern: /localStorage\.clear\s*\(|sessionStorage\.clear\s*\(/g,
                severity: 'low',
                message: 'Storage clearing detected - may affect user data',
                recommendation: 'Clear specific items instead of all storage'
            }
        ];
        
        // Check each pattern
        securityChecks.forEach(check => {
            const matches = code.match(check.pattern);
            if (matches) {
                // Check if pattern is explicitly allowed
                const isAllowed = allowPatterns.some(allowed => 
                    check.pattern.source.includes(allowed) || 
                    matches.some(match => match.includes(allowed))
                );
                
                if (!isAllowed) {
                    violations.push({
                        type: 'security_risk',
                        severity: check.severity,
                        message: check.message,
                        recommendation: check.recommendation,
                        matches: matches.length,
                        pattern: check.pattern.source
                    });
                }
            }
        });
        
        return { violations };
    }
    
    calculateScore(violations) {
        let score = 100;
        
        violations.forEach(violation => {
            switch (violation.severity) {
                case 'high':
                    score -= 25;
                    break;
                case 'medium':
                    score -= 15;
                    break;
                case 'low':
                    score -= 5;
                    break;
            }
        });
        
        return Math.max(0, score);
    }
}

module.exports = ValidateJSCommand;