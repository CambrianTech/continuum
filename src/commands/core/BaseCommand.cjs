/**
 * BaseCommand - Universal Command Interface with Distributed Capability System
 * 
 * Commands declare their implementations across different execution contexts:
 * - Local hardware (browser, python, server)
 * - Remote resources (supercomputers, cloud APIs)
 * - Paid services (per-token, per-compute-hour)
 * - Specialized hardware (GPU clusters, inference servers)
 */

class BaseCommand {
    /**
     * Get all available implementations for this command
     * Each implementation declares its capabilities, costs, and requirements
     */
    static getImplementations() {
        // Override in subclasses to declare implementations
        return [
            {
                name: 'local_default',
                provider: 'local',
                execution_context: 'browser',
                readyStatus: 'available',
                quality: 'medium',
                cost: { type: 'free' },
                ranking: 50
            }
        ];
    }

    /**
     * Implementation capability schema
     */
    static getImplementationSchema() {
        return {
            // Basic identification
            name: 'string',              // e.g., 'opera_devtools', 'gpt4_vision', 'local_llm'
            provider: 'string',          // e.g., 'local', 'openai', 'anthropic', 'supercomputer_x'
            execution_context: 'string', // e.g., 'browser', 'python', 'server', 'remote_gpu'
            
            // Capability status
            readyStatus: 'enum',         // 'available', 'degraded', 'unavailable', 'rate_limited'
            quality: 'enum',             // 'low', 'medium', 'high', 'premium'
            reliability: 'number',       // 0-100 success rate percentage
            
            // Performance characteristics
            latency: 'object',           // { min: 100, avg: 500, max: 2000 } ms
            throughput: 'object',        // { max_concurrent: 5, max_per_hour: 1000 }
            resource_usage: 'object',    // { cpu: 'low', memory: 'medium', gpu: 'high' }
            
            // Cost structure
            cost: 'object',              // See cost schema below
            
            // User experience impact
            ux_impact: 'enum',           // 'seamless', 'minor_delay', 'debug_window_required', 'user_approval_required'
            privacy_level: 'enum',       // 'local', 'encrypted_remote', 'cloud_processed'
            
            // Requirements and constraints
            requirements: 'array',       // ['internet', 'gpu', 'browser_open', 'payment_method']
            constraints: 'object',       // { max_file_size: '10MB', supported_formats: ['png', 'jpg'] }
            
            // Ranking for automatic selection
            ranking: 'number',           // 0-100, higher = more preferred
            
            // Dynamic status callback
            statusCheck: 'function'      // async () => { readyStatus, latency, cost }
        };
    }

    /**
     * Cost structure examples
     */
    static getCostExamples() {
        return {
            free: { type: 'free' },
            
            per_execution: {
                type: 'per_execution',
                amount: 0.01,
                currency: 'USD',
                description: '$0.01 per screenshot'
            },
            
            per_token: {
                type: 'per_token',
                input_cost: 0.000001,
                output_cost: 0.000002,
                currency: 'USD',
                description: '$0.000001 input, $0.000002 output per token'
            },
            
            per_compute_hour: {
                type: 'per_compute_hour',
                amount: 5.00,
                currency: 'USD',
                description: '$5.00 per GPU hour'
            },
            
            subscription: {
                type: 'subscription',
                tier: 'premium',
                included_quota: 1000,
                overage_cost: 0.001,
                description: '1000 executions/month, $0.001 per overage'
            },
            
            resource_share: {
                type: 'resource_share',
                gives: 'cpu_cycles',
                receives: 'gpu_access',
                description: 'Share CPU for GPU access'
            }
        };
    }

    /**
     * Select best implementation based on user preferences and current conditions
     */
    static async selectImplementation(params = {}, userPreferences = {}) {
        const implementations = await this.getAvailableImplementations();
        
        if (implementations.length === 0) {
            throw new Error(`No implementations available for ${this.metadata.name} command`);
        }

        // Apply user filters
        let candidates = implementations.filter(impl => {
            // Cost constraints
            if (userPreferences.maxCost && this.calculateCost(impl, params) > userPreferences.maxCost) {
                return false;
            }
            
            // Privacy requirements
            if (userPreferences.privacyLevel && impl.privacy_level !== userPreferences.privacyLevel) {
                return false;
            }
            
            // Quality requirements
            if (userPreferences.minQuality && this.qualityToNumber(impl.quality) < this.qualityToNumber(userPreferences.minQuality)) {
                return false;
            }
            
            return impl.readyStatus === 'available';
        });

        if (candidates.length === 0) {
            // Fallback to degraded implementations if no perfect matches
            candidates = implementations.filter(impl => impl.readyStatus === 'degraded');
        }

        if (candidates.length === 0) {
            throw new Error(`No usable implementations for ${this.metadata.name} command`);
        }

        // Sort by ranking (considering current status)
        candidates.sort((a, b) => {
            // Adjust ranking based on current status
            const aScore = a.ranking * (a.readyStatus === 'available' ? 1.0 : 0.7);
            const bScore = b.ranking * (b.readyStatus === 'available' ? 1.0 : 0.7);
            return bScore - aScore;
        });

        return candidates[0];
    }

    /**
     * Get current status of all implementations
     */
    static async getAvailableImplementations() {
        const implementations = this.getImplementations();
        const results = [];

        for (const impl of implementations) {
            try {
                let currentImpl = { ...impl };
                
                // Run dynamic status check if provided
                if (impl.statusCheck && typeof impl.statusCheck === 'function') {
                    const dynamicStatus = await impl.statusCheck();
                    currentImpl = { ...currentImpl, ...dynamicStatus };
                }
                
                results.push(currentImpl);
            } catch (error) {
                // Mark as unavailable if status check fails
                results.push({
                    ...impl,
                    readyStatus: 'unavailable',
                    error: error.message
                });
            }
        }

        return results;
    }

    static calculateCost(implementation, params) {
        const { cost } = implementation;
        
        switch (cost.type) {
            case 'free':
                return 0;
            case 'per_execution':
                return cost.amount;
            case 'per_token':
                const estimatedTokens = this.estimateTokens(params);
                return (estimatedTokens.input * cost.input_cost) + (estimatedTokens.output * cost.output_cost);
            default:
                return 0;
        }
    }

    static qualityToNumber(quality) {
        const map = { low: 1, medium: 2, high: 3, premium: 4 };
        return map[quality] || 0;
    }

    static estimateTokens(params) {
        // Override in subclasses to provide token estimation
        return { input: 100, output: 50 };
    }

    /**
     * INTERROGATION API: Get command status and capabilities
     */
    static async interrogate(params = {}, userPreferences = {}) {
        const implementations = await this.getAvailableImplementations();
        const selectedImpl = await this.selectImplementation(params, userPreferences);
        
        return {
            command: this.metadata?.name || this.name,
            total_implementations: implementations.length,
            available_implementations: implementations.filter(impl => impl.readyStatus === 'available').length,
            selected_implementation: selectedImpl,
            capabilities: {
                execution_contexts: [...new Set(implementations.map(impl => impl.execution_context))],
                providers: [...new Set(implementations.map(impl => impl.provider))],
                cost_models: [...new Set(implementations.map(impl => impl.cost.type))],
                quality_levels: [...new Set(implementations.map(impl => impl.quality))],
                privacy_levels: [...new Set(implementations.map(impl => impl.privacy_level))]
            },
            status_summary: {
                best_available: selectedImpl?.readyStatus || 'unavailable',
                estimated_cost: this.calculateCost(selectedImpl, params),
                ux_impact: selectedImpl?.ux_impact || 'unknown',
                quality: selectedImpl?.quality || 'unknown',
                latency: selectedImpl?.latency || { avg: 'unknown' }
            },
            all_implementations: implementations
        };
    }

    /**
     * INTERROGATION API: Check if command can fulfill specific requirements
     */
    static async canFulfill(requirements = {}) {
        const implementations = await this.getAvailableImplementations();
        
        const matching = implementations.filter(impl => {
            if (requirements.max_cost && this.calculateCost(impl, {}) > requirements.max_cost) return false;
            if (requirements.min_quality && this.qualityToNumber(impl.quality) < this.qualityToNumber(requirements.min_quality)) return false;
            if (requirements.privacy_level && impl.privacy_level !== requirements.privacy_level) return false;
            if (requirements.execution_context && impl.execution_context !== requirements.execution_context) return false;
            if (requirements.provider && impl.provider !== requirements.provider) return false;
            return impl.readyStatus === 'available';
        });
        
        return {
            can_fulfill: matching.length > 0,
            matching_implementations: matching.length,
            best_match: matching.length > 0 ? matching.sort((a, b) => b.ranking - a.ranking)[0] : null,
            requirements_checked: requirements
        };
    }

    /**
     * INTERROGATION API: Get real-time status of all implementations
     */
    static async getStatusReport() {
        const implementations = await this.getAvailableImplementations();
        
        const statusGroups = {
            available: implementations.filter(impl => impl.readyStatus === 'available'),
            degraded: implementations.filter(impl => impl.readyStatus === 'degraded'),
            unavailable: implementations.filter(impl => impl.readyStatus === 'unavailable'),
            rate_limited: implementations.filter(impl => impl.readyStatus === 'rate_limited')
        };
        
        return {
            command: this.metadata?.name || this.name,
            overall_status: statusGroups.available.length > 0 ? 'operational' : 
                           statusGroups.degraded.length > 0 ? 'degraded' : 'unavailable',
            status_breakdown: {
                available: statusGroups.available.length,
                degraded: statusGroups.degraded.length, 
                unavailable: statusGroups.unavailable.length,
                rate_limited: statusGroups.rate_limited.length
            },
            detailed_status: statusGroups,
            last_checked: new Date().toISOString()
        };
    }

    /**
     * Execute command with automatic implementation selection
     */
    static async execute(params, continuum, userPreferences = {}) {
        const implementation = await this.selectImplementation(params, userPreferences);
        
        console.log(`⚡ EXECUTING ${this.metadata?.name || this.name} via ${implementation.name} (${implementation.provider})`);
        if (implementation.cost.type !== 'free') {
            const cost = this.calculateCost(implementation, params);
            console.log(`💰 Estimated cost: ${cost} ${implementation.cost.currency || 'USD'}`);
        }
        
        // Route to specific implementation
        return await this.executeImplementation(implementation, params, continuum);
    }

    /**
     * Route execution to specific implementation
     */
    static async executeImplementation(implementation, params, continuum) {
        const { execution_context, provider } = implementation;
        
        switch (execution_context) {
            case 'browser':
                return await this.executeBrowser(implementation, params, continuum);
            case 'python':
                return await this.executePython(implementation, params, continuum);
            case 'server':
                return await this.executeServer(implementation, params, continuum);
            case 'remote_gpu':
            case 'supercomputer':
                return await this.executeRemote(implementation, params, continuum);
            default:
                throw new Error(`Unknown execution context: ${execution_context}`);
        }
    }

    // Implementation-specific execution methods (override in subclasses)
    static async executeBrowser(implementation, params, continuum) {
        throw new Error(`Browser execution not implemented for ${this.metadata.name}`);
    }

    static async executePython(implementation, params, continuum) {
        throw new Error(`Python execution not implemented for ${this.metadata.name}`);
    }

    static async executeServer(implementation, params, continuum) {
        throw new Error(`Server execution not implemented for ${this.metadata.name}`);
    }

    static async executeRemote(implementation, params, continuum) {
        throw new Error(`Remote execution not implemented for ${this.metadata.name}`);
    }

    // ===== LEGACY COMPATIBILITY METHODS =====
    // These methods provide compatibility with older command implementations
    // that haven't been migrated to the new Universal Lambda architecture

    /**
     * Parse parameters from various input formats
     */
    static parseParams(params) {
        try {
            if (!params) return {};
            
            // If already an object, return as-is
            if (typeof params === 'object' && !Array.isArray(params)) {
                return params;
            }
            
            // If string, parse as JSON
            if (typeof params === 'string') {
                // Handle empty string
                if (params.trim() === '') {
                    return {};
                }
                return JSON.parse(params);
            }
            
            console.warn(`⚠️ Invalid parameter type: ${typeof params}. Expected object or JSON string.`);
            return {};
        } catch (error) {
            console.warn(`⚠️ Parameter parsing failed: ${error.message}. Returning empty object.`);
            return {};
        }
    }

    /**
     * Validate parameters against command definition
     */
    static validateParams(params, definition) {
        try {
            if (!definition || !definition.parameters) {
                return { valid: true, errors: [] };
            }

            const errors = [];
            const paramDefinition = definition.parameters;

            // Check required parameters
            for (const [paramName, paramConfig] of Object.entries(paramDefinition)) {
                if (paramConfig.required && !(paramName in params)) {
                    errors.push(`Missing required parameter: ${paramName}`);
                }
                
                // Type validation
                if (paramName in params && paramConfig.type) {
                    const value = params[paramName];
                    const expectedType = paramConfig.type;
                    
                    if (!this.isValidType(value, expectedType)) {
                        errors.push(`Parameter '${paramName}' must be ${expectedType}, got ${typeof value}`);
                    }
                }
            }

            return {
                valid: errors.length === 0,
                errors
            };
        } catch (error) {
            return {
                valid: false,
                errors: [`Validation error: ${error.message}`]
            };
        }
    }

    /**
     * Type checking helper
     */
    static isValidType(value, expectedType) {
        switch (expectedType) {
            case 'string':
                return typeof value === 'string';
            case 'number':
                return typeof value === 'number' && !isNaN(value);
            case 'boolean':
                return typeof value === 'boolean';
            case 'array':
                return Array.isArray(value);
            case 'object':
                return typeof value === 'object' && !Array.isArray(value) && value !== null;
            default:
                return true; // Unknown types pass validation
        }
    }

    /**
     * Create success result object
     */
    static createSuccessResult(data, message = 'Success') {
        return {
            success: true,
            message,
            data,
            timestamp: new Date().toISOString()
        };
    }

    /**
     * Create error result object
     */
    static createErrorResult(message, error = null) {
        return {
            success: false,
            message,
            error,
            timestamp: new Date().toISOString()
        };
    }

    /**
     * Legacy getDefinition method for commands that haven't migrated to metadata
     */
    static getDefinition() {
        // Override in subclasses or provide metadata
        if (this.metadata) {
            return this.metadata;
        }
        throw new Error('getDefinition or metadata must be implemented by subclass');
    }
}

module.exports = BaseCommand;