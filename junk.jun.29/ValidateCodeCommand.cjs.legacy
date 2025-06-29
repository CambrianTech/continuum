/**
 * Universal ValidateCode Command
 * Integrates existing validation APIs for multiple languages
 * Works seamlessly across all clients - saves time on syntax/security checks
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class ValidateCodeCommand extends BaseCommand {
    constructor() {
        super();
        this.commandName = 'validate_code';
        this.description = 'Universal code validation using existing APIs and tools';
        this.parameters = {
            code: {
                type: 'string',
                required: true,
                description: 'Code to validate'
            },
            language: {
                type: 'string',
                required: true,
                enum: ['javascript', 'python', 'typescript', 'json', 'css', 'html'],
                description: 'Programming language'
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
                description: 'Strict mode - reject any warnings'
            },
            fix: {
                type: 'boolean',
                required: false,
                default: false,
                description: 'Attempt to auto-fix issues where possible'
            }
        };
    }

    static getDefinition() {
        return {
            name: 'validate_code',
            description: 'Universal code validation using existing APIs and tools',
            parameters: {
                code: { type: 'string', required: true, description: 'Code to validate' },
                language: { type: 'string', required: true, enum: ['javascript', 'python', 'typescript', 'json', 'css', 'html'], description: 'Programming language' },
                context: { type: 'object', required: false, description: 'Validation context and permissions' },
                strict: { type: 'boolean', required: false, default: false, description: 'Strict mode - reject any warnings' },
                fix: { type: 'boolean', required: false, default: false, description: 'Attempt to auto-fix issues where possible' }
            }
        };
    }

    static async execute(params, continuum) {
        const { code, language, context = {}, strict = false, fix = false } = params;
        
        console.log(`🔍 ValidateCode: ${language} (${code.length} chars, strict=${strict}, fix=${fix})`);
        
        try {
            // Route to appropriate validator based on language
            switch (language.toLowerCase()) {
                case 'javascript':
                case 'js':
                    return await this.validateJavaScript(code, context, strict, fix, continuum);
                
                case 'python':
                case 'py':
                    return await this.validatePython(code, context, strict, fix, continuum);
                
                case 'typescript':
                case 'ts':
                    return await this.validateTypeScript(code, context, strict, fix, continuum);
                
                case 'json':
                    return await this.validateJSON(code, context, strict, fix);
                
                case 'css':
                    return await this.validateCSS(code, context, strict, fix);
                
                case 'html':
                    return await this.validateHTML(code, context, strict, fix);
                
                default:
                    return this.createErrorResult(`Unsupported language: ${language}. Supported: javascript, python, typescript, json, css, html`);
            }
            
        } catch (error) {
            console.error('❌ ValidateCode execution error:', error);
            return this.createErrorResult(`Validation failed: ${error.message}`);
        }
    }
    
    async validateJavaScript(code, context, strict, fix, continuum) {
        const issues = [];
        let fixedCode = code;
        
        // 1. Syntax validation using Node.js built-ins
        try {
            new Function(code);
            console.log('✅ JavaScript syntax valid');
        } catch (syntaxError) {
            issues.push({
                type: 'syntax',
                severity: 'high',
                message: `Syntax error: ${syntaxError.message}`,
                line: this.extractLineNumber(syntaxError.message),
                fixable: false
            });
        }
        
        // 2. Security pattern analysis
        const securityIssues = this.analyzeJavaScriptSecurity(code, context);
        issues.push(...securityIssues);
        
        // 3. Best practices check
        const practiceIssues = this.analyzeJavaScriptPractices(code, context);
        issues.push(...practiceIssues);
        
        // 4. Use ProtocolSheriff if available
        if (continuum?.protocolSheriff) {
            try {
                const sheriffResult = await continuum.protocolSheriff.validateCode(code, {
                    language: 'javascript',
                    context: context
                });
                if (sheriffResult?.violations) {
                    issues.push(...sheriffResult.violations.map(v => ({
                        ...v,
                        source: 'protocol_sheriff'
                    })));
                }
            } catch (sheriffError) {
                console.warn('⚠️ ProtocolSheriff unavailable:', sheriffError.message);
            }
        }
        
        // 5. Auto-fix if requested
        if (fix && issues.some(i => i.fixable)) {
            fixedCode = this.autoFixJavaScript(code, issues.filter(i => i.fixable));
        }
        
        return this.formatValidationResult(issues, strict, {
            language: 'javascript',
            originalCode: code,
            fixedCode: fix ? fixedCode : null,
            recommendations: this.generateJavaScriptRecommendations(issues)
        });
    }
    
    async validatePython(code, context, strict, fix, continuum) {
        const issues = [];
        let fixedCode = code;
        
        // 1. Syntax validation using ast module (if available)
        const syntaxCheck = await this.checkPythonSyntax(code);
        if (!syntaxCheck.valid) {
            issues.push({
                type: 'syntax',
                severity: 'high',
                message: syntaxCheck.error,
                line: syntaxCheck.line,
                fixable: false
            });
        }
        
        // 2. Security analysis
        const securityIssues = this.analyzePythonSecurity(code, context);
        issues.push(...securityIssues);
        
        // 3. Style and best practices (basic checks)
        const styleIssues = this.analyzePythonStyle(code);
        issues.push(...styleIssues);
        
        // 4. Use ProtocolSheriff for AI analysis
        if (continuum?.protocolSheriff) {
            try {
                const sheriffResult = await continuum.protocolSheriff.validateCode(code, {
                    language: 'python',
                    context: context
                });
                if (sheriffResult?.violations) {
                    issues.push(...sheriffResult.violations.map(v => ({
                        ...v,
                        source: 'protocol_sheriff'
                    })));
                }
            } catch (sheriffError) {
                console.warn('⚠️ ProtocolSheriff unavailable:', sheriffError.message);
            }
        }
        
        return this.formatValidationResult(issues, strict, {
            language: 'python',
            originalCode: code,
            fixedCode: fix ? fixedCode : null,
            recommendations: this.generatePythonRecommendations(issues)
        });
    }
    
    async validateJSON(code, context, strict, fix) {
        const issues = [];
        let fixedCode = code;
        
        try {
            const parsed = JSON.parse(code);
            console.log('✅ JSON syntax valid');
            
            // Additional JSON analysis
            if (typeof parsed === 'object' && parsed !== null) {
                const analysis = this.analyzeJSONStructure(parsed, context);
                issues.push(...analysis);
            }
            
        } catch (jsonError) {
            issues.push({
                type: 'syntax',
                severity: 'high',
                message: `JSON parsing error: ${jsonError.message}`,
                fixable: true
            });
            
            if (fix) {
                fixedCode = this.attemptJSONFix(code);
            }
        }
        
        return this.formatValidationResult(issues, strict, {
            language: 'json',
            originalCode: code,
            fixedCode: fix ? fixedCode : null
        });
    }
    
    // Security analysis for JavaScript
    analyzeJavaScriptSecurity(code, context) {
        const issues = [];
        
        const securityPatterns = [
            {
                pattern: /eval\s*\(/g,
                severity: 'high',
                message: 'eval() usage - code injection risk',
                recommendation: 'Use JSON.parse() or refactor to avoid dynamic evaluation'
            },
            {
                pattern: /Function\s*\(/g,
                severity: 'high',
                message: 'Function constructor - code injection risk',
                recommendation: 'Use declared functions or arrow functions'
            },
            {
                pattern: /document\.write\s*\(/g,
                severity: 'medium',
                message: 'document.write() usage',
                recommendation: 'Use DOM manipulation methods'
            },
            {
                pattern: /\.innerHTML\s*=/g,
                severity: 'medium',
                message: 'innerHTML assignment - XSS risk',
                recommendation: 'Use textContent or createElement'
            }
        ];
        
        securityPatterns.forEach(check => {
            const matches = code.match(check.pattern);
            if (matches) {
                issues.push({
                    type: 'security',
                    severity: check.severity,
                    message: check.message,
                    recommendation: check.recommendation,
                    occurrences: matches.length
                });
            }
        });
        
        return issues;
    }
    
    // Security analysis for Python
    analyzePythonSecurity(code, context) {
        const issues = [];
        
        const pythonSecurityPatterns = [
            {
                pattern: /exec\s*\(/g,
                severity: 'high',
                message: 'exec() usage - code injection risk'
            },
            {
                pattern: /eval\s*\(/g,
                severity: 'high',
                message: 'eval() usage - code injection risk'
            },
            {
                pattern: /os\.system\s*\(/g,
                severity: 'high',
                message: 'os.system() usage - command injection risk'
            },
            {
                pattern: /subprocess\.call\s*\(/g,
                severity: 'medium',
                message: 'subprocess.call() - validate input carefully'
            },
            {
                pattern: /pickle\.loads?\s*\(/g,
                severity: 'medium',
                message: 'pickle usage - potential code execution'
            }
        ];
        
        pythonSecurityPatterns.forEach(check => {
            const matches = code.match(check.pattern);
            if (matches) {
                issues.push({
                    type: 'security',
                    severity: check.severity,
                    message: check.message,
                    occurrences: matches.length
                });
            }
        });
        
        return issues;
    }
    
    // Check Python syntax (mock implementation - would use actual Python AST)
    async checkPythonSyntax(code) {
        // In real implementation, would use python -m py_compile or ast module
        // For now, basic checks
        
        const lines = code.split('\n');
        let indentLevel = 0;
        
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i].trim();
            if (!line || line.startsWith('#')) continue;
            
            // Basic syntax checks
            if (line.endsWith(':') && !line.match(/^(if|for|while|def|class|try|except|finally|with|else|elif)/)) {
                return {
                    valid: false,
                    error: `Unexpected colon at line ${i + 1}`,
                    line: i + 1
                };
            }
        }
        
        return { valid: true };
    }
    
    formatValidationResult(issues, strict, metadata) {
        const highSeverityIssues = issues.filter(i => i.severity === 'high');
        const mediumSeverityIssues = issues.filter(i => i.severity === 'medium');
        const lowSeverityIssues = issues.filter(i => i.severity === 'low');
        
        const hasHighSeverity = highSeverityIssues.length > 0;
        const isValid = !hasHighSeverity && (strict ? issues.length === 0 : true);
        
        const score = Math.max(0, 100 - (highSeverityIssues.length * 25) - (mediumSeverityIssues.length * 10) - (lowSeverityIssues.length * 5));
        
        if (!isValid) {
            const primaryIssue = highSeverityIssues[0] || issues[0];
            return this.createErrorResult(
                strict ? 'Code rejected in strict mode' : `Validation failed: ${primaryIssue.message}`,
                {
                    score,
                    issues,
                    summary: {
                        high: highSeverityIssues.length,
                        medium: mediumSeverityIssues.length,
                        low: lowSeverityIssues.length
                    },
                    ...metadata
                }
            );
        }
        
        return this.createSuccessResult({
            score,
            issues,
            summary: {
                high: highSeverityIssues.length,
                medium: mediumSeverityIssues.length,
                low: lowSeverityIssues.length
            },
            ...metadata
        }, `Code validation passed (score: ${score}/100)`);
    }
    
    extractLineNumber(errorMessage) {
        const lineMatch = errorMessage.match(/line (\d+)/);
        return lineMatch ? parseInt(lineMatch[1]) : null;
    }
    
    generateJavaScriptRecommendations(issues) {
        const recommendations = [];
        
        if (issues.some(i => i.message.includes('eval'))) {
            recommendations.push('Replace eval() with JSON.parse() for data or refactor logic');
        }
        
        if (issues.some(i => i.message.includes('innerHTML'))) {
            recommendations.push('Use textContent or DOM methods instead of innerHTML');
        }
        
        if (issues.some(i => i.message.includes('document.body'))) {
            recommendations.push('Use specific selectors like querySelector("#main-content")');
        }
        
        return recommendations;
    }
    
    generatePythonRecommendations(issues) {
        const recommendations = [];
        
        if (issues.some(i => i.message.includes('exec') || i.message.includes('eval'))) {
            recommendations.push('Avoid dynamic code execution - use data structures instead');
        }
        
        if (issues.some(i => i.message.includes('os.system'))) {
            recommendations.push('Use subprocess.run() with shell=False for safer command execution');
        }
        
        return recommendations;
    }
}

module.exports = ValidateCodeCommand;