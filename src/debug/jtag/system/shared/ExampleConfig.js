/**
 * Example Configuration Manager
 *
 * Centralized management of JTAG example configurations.
 * Allows switching between test-bench, widget-ui, and other examples
 * with dynamic port and feature configuration.
 */
import { readFileSync, existsSync } from 'fs';
import { join } from 'path';
class ExampleConfigManager {
    constructor() {
        this.configCache = null;
        this.configPath = join(__dirname, '../../config/examples.json');
    }
    static getInstance() {
        if (!ExampleConfigManager.instance) {
            ExampleConfigManager.instance = new ExampleConfigManager();
        }
        return ExampleConfigManager.instance;
    }
    /**
     * Load examples configuration from file
     */
    loadConfig() {
        if (!this.configCache) {
            try {
                if (!existsSync(this.configPath)) {
                    throw new Error(`Examples configuration file not found: ${this.configPath}`);
                }
                const rawConfig = JSON.parse(readFileSync(this.configPath, 'utf-8'));
                // Apply environment variable overrides
                const config = {
                    ...rawConfig,
                    active_example: process.env.JTAG_ACTIVE_EXAMPLE || rawConfig.active_example
                };
                // Apply port overrides from environment
                if (process.env.JTAG_EXAMPLE_HTTP_PORT) {
                    const activeExample = config.examples[config.active_example];
                    if (activeExample) {
                        config.examples[config.active_example] = {
                            ...activeExample,
                            ports: {
                                ...activeExample.ports,
                                http_server: parseInt(process.env.JTAG_EXAMPLE_HTTP_PORT)
                            }
                        };
                    }
                }
                this.configCache = config;
                console.log(`📋 Examples configuration loaded (active: ${config.active_example})`);
            }
            catch (error) {
                throw new Error(`Failed to load examples configuration: ${error}`);
            }
        }
        return this.configCache;
    }
    /**
     * Get the currently active example configuration
     */
    getActiveExample() {
        const config = this.loadConfig();
        const activeExample = config.examples[config.active_example];
        if (!activeExample) {
            throw new Error(`Active example '${config.active_example}' not found in configuration`);
        }
        return activeExample;
    }
    /**
     * Get configuration for a specific example
     */
    getExample(name) {
        const config = this.loadConfig();
        const example = config.examples[name];
        if (!example) {
            throw new Error(`Example '${name}' not found in configuration`);
        }
        return example;
    }
    /**
     * Get all available example names
     */
    getAvailableExamples() {
        const config = this.loadConfig();
        return Object.keys(config.examples);
    }
    /**
     * Get the active example name
     */
    getActiveExampleName() {
        const config = this.loadConfig();
        return config.active_example;
    }
    /**
     * Get port configuration for active example
     */
    getActivePorts() {
        const activeExample = this.getActiveExample();
        return activeExample.ports;
    }
    /**
     * Get common configuration (shared across all examples)
     */
    getCommonConfig() {
        const config = this.loadConfig();
        return config.common;
    }
    /**
     * Switch active example (runtime switching)
     */
    switchActiveExample(name) {
        const config = this.loadConfig();
        if (!config.examples[name]) {
            throw new Error(`Cannot switch to unknown example: ${name}`);
        }
        config.active_example = name;
        console.log(`🔄 Switched active example to: ${name}`);
    }
    /**
     * Check if a specific feature is enabled for the active example
     */
    isFeatureEnabled(feature) {
        const activeExample = this.getActiveExample();
        return activeExample.features[feature];
    }
    /**
     * Get full absolute path for active example directory
     */
    getActiveExamplePath() {
        const activeExample = this.getActiveExample();
        return join(__dirname, '../../', activeExample.paths.directory);
    }
    /**
     * Clear configuration cache (useful for testing)
     */
    clearCache() {
        this.configCache = null;
        console.log('🔄 Example configuration cache cleared');
    }
    /**
     * Validate configuration structure
     */
    validateConfig() {
        try {
            const config = this.loadConfig();
            const errors = [];
            // Check if active_example exists
            if (!config.examples[config.active_example]) {
                errors.push(`Active example '${config.active_example}' not found in examples`);
            }
            // Validate each example
            for (const [name, example] of Object.entries(config.examples)) {
                if (!example.ports.http_server || !example.ports.websocket_server) {
                    errors.push(`Example '${name}' missing required port configuration`);
                }
                if (!example.paths.directory || !example.paths.html_file) {
                    errors.push(`Example '${name}' missing required path configuration`);
                }
            }
            return { valid: errors.length === 0, errors };
        }
        catch (error) {
            return { valid: false, errors: [String(error)] };
        }
    }
}
// Public API
export function getActiveExample() {
    return ExampleConfigManager.getInstance().getActiveExample();
}
export function getExample(name) {
    return ExampleConfigManager.getInstance().getExample(name);
}
export function getActiveExampleName() {
    return ExampleConfigManager.getInstance().getActiveExampleName();
}
export function getActivePorts() {
    return ExampleConfigManager.getInstance().getActivePorts();
}
export function getCommonConfig() {
    return ExampleConfigManager.getInstance().getCommonConfig();
}
export function switchActiveExample(name) {
    ExampleConfigManager.getInstance().switchActiveExample(name);
}
export function isFeatureEnabled(feature) {
    return ExampleConfigManager.getInstance().isFeatureEnabled(feature);
}
export function getActiveExamplePath() {
    return ExampleConfigManager.getInstance().getActiveExamplePath();
}
export function clearExampleConfigCache() {
    ExampleConfigManager.getInstance().clearCache();
}
export function validateExampleConfig() {
    return ExampleConfigManager.getInstance().validateConfig();
}
export function getAvailableExamples() {
    return ExampleConfigManager.getInstance().getAvailableExamples();
}
