/**
 * Validation Pipeline - Multi-layer code validation system
 * Layer 1: Basic validation (syntax, security patterns) - fast rejection
 * Layer 2: ProtocolSheriff (AI-powered analysis) - smart approval
 * Layer 3: Context-aware execution - safe execution
 */

class ValidationPipeline {
    constructor(protocolSheriff = null, commandRegistry = null) {
        this.protocolSheriff = protocolSheriff;
        this.commandRegistry = commandRegistry;
        this.stats = {
            totalValidations: 0,
            basicRejects: 0,
            sheriffRejects: 0,
            approved: 0,
            byLanguage: {}
        };
    }

    /**
     * Main validation entry point - automatically called for all incoming code
     */
    async validateIncomingCode(codeRequest) {
        const { code, language, context = {}, source = 'unknown' } = codeRequest;
        
        console.log(`🛡️ ValidationPipeline: Incoming ${language} code from ${source} (${code.length} chars)`);
        this.stats.totalValidations++;
        
        const startTime = Date.now();
        
        try {
            // LAYER 1: Basic Validation (fast rejection)
            console.log('🔍 Layer 1: Basic validation...');
            const basicResult = await this.runBasicValidation(code, language, context);
            
            if (!basicResult.passed) {
                this.stats.basicRejects++;
                this.updateLanguageStats(language, 'basic_reject');
                
                console.log(`❌ REJECTED at Layer 1: ${basicResult.reason}`);
                return this.createRejectionResult('basic_validation', basicResult.reason, basicResult.details);
            }
            
            console.log('✅ Layer 1: Passed basic validation');
            
            // LAYER 2: ProtocolSheriff (AI analysis)
            console.log('🤖 Layer 2: ProtocolSheriff analysis...');
            const sheriffResult = await this.runSheriffValidation(code, language, context, basicResult.metadata);
            
            if (!sheriffResult.passed) {
                this.stats.sheriffRejects++;
                this.updateLanguageStats(language, 'sheriff_reject');
                
                console.log(`❌ REJECTED at Layer 2: ${sheriffResult.reason}`);
                return this.createRejectionResult('protocol_sheriff', sheriffResult.reason, sheriffResult.details);
            }
            
            console.log('✅ Layer 2: Passed ProtocolSheriff review');
            
            // LAYER 3: Final approval with execution context
            const executionContext = this.prepareExecutionContext(code, language, context, {
                basicValidation: basicResult,
                sheriffValidation: sheriffResult
            });
            
            this.stats.approved++;
            this.updateLanguageStats(language, 'approved');
            
            const duration = Date.now() - startTime;
            console.log(`✅ APPROVED: ${language} code validated in ${duration}ms`);
            
            return this.createApprovalResult(executionContext);
            
        } catch (error) {
            console.error('❌ Validation pipeline error:', error);
            return this.createRejectionResult('pipeline_error', error.message, { error: error.stack });
        }
    }
    
    /**
     * Layer 1: Basic validation - syntax, security patterns, size limits
     */
    async runBasicValidation(code, language, context) {
        const issues = [];
        const metadata = { language, codeLength: code.length };
        
        // 1. Size limits
        if (code.length > 100000) {
            return {
                passed: false,
                reason: 'Code too large - maximum 100,000 characters allowed',
                details: { size: code.length, limit: 100000 }
            };
        }
        
        // 2. Language-specific basic checks
        switch (language.toLowerCase()) {
            case 'javascript':
            case 'js':
                return await this.validateJavaScriptBasics(code, context);
                
            case 'python':
            case 'py':
                return await this.validatePythonBasics(code, context);
                
            case 'json':
                return this.validateJSONBasics(code, context);
                
            default:
                // For unknown languages, do minimal validation
                return {
                    passed: true,
                    metadata: { language: 'unknown', basicChecks: 'minimal' }
                };
        }
    }
    
    async validateJavaScriptBasics(code, context) {
        const issues = [];
        
        // 1. Syntax check
        try {
            new Function(code);
        } catch (syntaxError) {
            return {
                passed: false,
                reason: `JavaScript syntax error: ${syntaxError.message}`,
                details: { type: 'syntax', error: syntaxError.message }
            };
        }
        
        // 2. Critical security patterns (immediate rejection)
        const criticalPatterns = [
            { pattern: /eval\s*\(/g, reason: 'eval() usage - immediate security risk' },
            { pattern: /Function\s*\(/g, reason: 'Function constructor - code injection risk' },
            { pattern: /document\.write\s*\(/g, reason: 'document.write() - can break page structure' }
        ];
        
        for (const check of criticalPatterns) {
            if (check.pattern.test(code)) {
                return {
                    passed: false,
                    reason: check.reason,
                    details: { type: 'security', pattern: check.pattern.source }
                };
            }
        }
        
        // 3. Context-specific checks
        if (!context.allowBodyAccess && /document\.body/.test(code)) {
            issues.push({ type: 'context', message: 'document.body access without permission' });
        }
        
        return {
            passed: true,
            metadata: { 
                language: 'javascript',
                syntaxValid: true,
                securityBasics: 'passed',
                contextIssues: issues.length
            }
        };
    }
    
    async validatePythonBasics(code, context) {
        // 1. Basic Python security patterns
        const criticalPatterns = [
            { pattern: /exec\s*\(/g, reason: 'exec() usage - code injection risk' },
            { pattern: /eval\s*\(/g, reason: 'eval() usage - code injection risk' },
            { pattern: /os\.system\s*\(/g, reason: 'os.system() - command injection risk' },
            { pattern: /__import__\s*\(/g, reason: '__import__() - dynamic import risk' }
        ];
        
        for (const check of criticalPatterns) {
            if (check.pattern.test(code)) {
                return {
                    passed: false,
                    reason: check.reason,
                    details: { type: 'security', pattern: check.pattern.source }
                };
            }
        }
        
        // 2. Basic syntax check (simple)
        const lines = code.split('\n');
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i].trim();
            if (line && !line.startsWith('#')) {
                // Check for obvious syntax issues
                if (line.includes(';;;') || line.match(/^[0-9]+[a-zA-Z]/)) {
                    return {
                        passed: false,
                        reason: `Suspicious syntax at line ${i + 1}`,
                        details: { type: 'syntax', line: i + 1, content: line.substring(0, 50) }
                    };
                }
            }
        }
        
        return {
            passed: true,
            metadata: { 
                language: 'python',
                basicSecurity: 'passed',
                lines: lines.length
            }
        };
    }
    
    validateJSONBasics(code, context) {
        try {
            const parsed = JSON.parse(code);
            return {
                passed: true,
                metadata: { 
                    language: 'json',
                    type: typeof parsed,
                    keys: typeof parsed === 'object' ? Object.keys(parsed).length : 0
                }
            };
        } catch (jsonError) {
            return {
                passed: false,
                reason: `JSON parsing error: ${jsonError.message}`,
                details: { type: 'syntax', error: jsonError.message }
            };
        }
    }
    
    /**
     * Layer 2: ProtocolSheriff validation - AI-powered analysis
     */
    async runSheriffValidation(code, language, context, basicMetadata) {
        if (!this.protocolSheriff) {
            console.log('⚠️ ProtocolSheriff not available, skipping AI validation');
            return {
                passed: true,
                metadata: { sheriffAvailable: false }
            };
        }
        
        try {
            const sheriffRequest = {
                code: code,
                language: language,
                context: {
                    ...context,
                    basicValidation: basicMetadata,
                    purpose: context.purpose || 'general',
                    agentRole: context.agentRole || 'code_validator'
                }
            };
            
            const result = await this.protocolSheriff.validateCode(sheriffRequest);
            
            if (!result.isValid) {
                return {
                    passed: false,
                    reason: `ProtocolSheriff rejection: ${result.violations[0] || 'Code policy violation'}`,
                    details: {
                        violations: result.violations,
                        score: result.score,
                        recommendation: result.recommendation
                    }
                };
            }
            
            return {
                passed: true,
                metadata: {
                    sheriffScore: result.score,
                    violations: result.violations,
                    recommendation: result.recommendation
                }
            };
            
        } catch (sheriffError) {
            console.error('⚠️ ProtocolSheriff error:', sheriffError.message);
            // Fail open - don't block on sheriff errors
            return {
                passed: true,
                metadata: { sheriffError: sheriffError.message }
            };
        }
    }
    
    prepareExecutionContext(code, language, originalContext, validationResults) {
        return {
            code: code,
            language: language,
            originalContext: originalContext,
            validationPassed: true,
            validationResults: validationResults,
            executionPermissions: this.calculateExecutionPermissions(validationResults),
            timestamp: new Date().toISOString(),
            pipelineVersion: '1.0.0'
        };
    }
    
    calculateExecutionPermissions(validationResults) {
        const permissions = {
            allowDOMAccess: false,
            allowNetworkAccess: false,
            allowFileAccess: false,
            allowSystemCalls: false,
            maxExecutionTime: 5000 // ms
        };
        
        // Adjust permissions based on validation results
        const sheriffScore = validationResults.sheriffValidation?.metadata?.sheriffScore || 0;
        
        if (sheriffScore >= 90) {
            permissions.allowDOMAccess = true;
            permissions.maxExecutionTime = 10000;
        }
        
        if (sheriffScore >= 95) {
            permissions.allowNetworkAccess = true;
        }
        
        return permissions;
    }
    
    createRejectionResult(layer, reason, details = {}) {
        return {
            validated: false,
            rejected: true,
            rejectionLayer: layer,
            reason: reason,
            details: details,
            timestamp: new Date().toISOString()
        };
    }
    
    createApprovalResult(executionContext) {
        return {
            validated: true,
            approved: true,
            executionContext: executionContext,
            timestamp: new Date().toISOString()
        };
    }
    
    updateLanguageStats(language, result) {
        if (!this.stats.byLanguage[language]) {
            this.stats.byLanguage[language] = {
                total: 0,
                basic_reject: 0,
                sheriff_reject: 0,
                approved: 0
            };
        }
        
        this.stats.byLanguage[language].total++;
        this.stats.byLanguage[language][result]++;
    }
    
    getValidationStats() {
        return {
            ...this.stats,
            rejectionRate: ((this.stats.basicRejects + this.stats.sheriffRejects) / this.stats.totalValidations * 100).toFixed(2),
            approvalRate: (this.stats.approved / this.stats.totalValidations * 100).toFixed(2)
        };
    }
}

module.exports = ValidationPipeline;