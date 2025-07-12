/**
 * Core Module Utilities
 * 
 * Utility functions for module management, testing, and operations
 * Separated from module-discovery for clean organization
 */

import type { ModuleDependency } from './discovery.js';

/**
 * Utility functions for common module operations
 */
export class ModuleUtils {
  /**
   * Create mock instances for testing using spread patterns
   */
  static createMockInstances<T extends Record<string, ModuleDependency>>(
    dependencies: T,
    overrides: Partial<Record<keyof T, any>> = {}
  ): Record<keyof T, any> {
    const mocks: Record<string, any> = {};
    
    for (const [name, spec] of Object.entries(dependencies)) {
      mocks[name] = {
        name,
        type: spec.type,
        start: async () => {},
        stop: async () => {},
        sendMessage: async () => ({ success: true, data: {} }),
        ...overrides[name as keyof T] // Spread override for specific mocks
      };
    }
    
    return mocks as Record<keyof T, any>;
  }

  /**
   * Calculate startup order based on dependencies
   */
  static calculateStartupOrder<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): (keyof T)[] {
    // Simple implementation - can be enhanced with dependency graph analysis
    const order: (keyof T)[] = [];
    const names = Object.keys(dependencies) as (keyof T)[];
    
    // Add required dependencies first
    const required = names.filter(name => dependencies[name].required);
    order.push(...required);
    
    // Add optional dependencies
    const optional = names.filter(name => !dependencies[name].required);
    order.push(...optional);
    
    return order;
  }

  /**
   * Get shutdown order (reverse of startup)
   */
  static calculateShutdownOrder<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): (keyof T)[] {
    return ModuleUtils.calculateStartupOrder(dependencies).reverse();
  }

  /**
   * Filter dependencies by type
   */
  static filterByType<T extends Record<string, ModuleDependency>>(
    dependencies: T,
    type: string
  ): Record<string, ModuleDependency> {
    return Object.fromEntries(
      Object.entries(dependencies).filter(([, spec]) => spec.type === type)
    );
  }

  /**
   * Get required dependencies only
   */
  static getRequired<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): Record<string, ModuleDependency> {
    return Object.fromEntries(
      Object.entries(dependencies).filter(([, spec]) => spec.required)
    );
  }

  /**
   * Get optional dependencies only
   */
  static getOptional<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): Record<string, ModuleDependency> {
    return Object.fromEntries(
      Object.entries(dependencies).filter(([, spec]) => !spec.required)
    );
  }

  /**
   * Merge dependency configurations with overrides
   */
  static mergeConfigs<T extends Record<string, ModuleDependency>>(
    dependencies: T,
    configOverrides: Partial<Record<keyof T, any>>
  ): Record<keyof T, any> {
    const merged: Record<string, any> = {};
    
    for (const [name, spec] of Object.entries(dependencies)) {
      merged[name] = {
        ...spec.config,
        ...configOverrides[name as keyof T]
      };
    }
    
    return merged as Record<keyof T, any>;
  }

  /**
   * Validate dependency structure
   */
  static validateDependencies<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): { valid: boolean; errors: string[] } {
    const errors: string[] = [];
    
    for (const [name, spec] of Object.entries(dependencies)) {
      if (!spec.name) {
        errors.push(`Dependency '${name}' missing name field`);
      }
      
      if (!spec.type) {
        errors.push(`Dependency '${name}' missing type field`);
      }
      
      if (typeof spec.required !== 'boolean') {
        errors.push(`Dependency '${name}' missing or invalid required field`);
      }
    }
    
    return {
      valid: errors.length === 0,
      errors
    };
  }
}