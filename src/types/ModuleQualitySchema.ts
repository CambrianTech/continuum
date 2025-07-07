/**
 * Module Quality Schema - TypeScript definitions for per-module quality standards
 * 
 * This schema defines the standardized way modules declare their quality requirements
 * in their package.json files under the "continuum.quality" field.
 */

/**
 * Module graduation status - AUTOMATED ONLY, cannot be manually edited
 * The quality enforcement system automatically determines and updates these statuses
 */
export enum ModuleGraduationStatus {
  /** PERFECT - Zero tolerance enforcement, production ready */
  GRADUATED = 'graduated',
  
  /** READY - All quality standards met, ready for graduation */
  CANDIDATE = 'candidate', 
  
  /** DEVELOPMENT - Work in progress, development flexibility */
  WHITELISTED = 'whitelisted',
  
  /** DEGRADED - Quality regression detected, requires immediate attention */
  DEGRADED = 'degraded',
  
  /** BROKEN - Critical failures, completely non-functional */
  BROKEN = 'broken',
  
  /** UNKNOWN - Not yet analyzed by quality system */
  UNKNOWN = 'unknown'
}

/**
 * ESLint enforcement level
 */
export enum ESLintLevel {
  /** Zero tolerance - must pass completely */
  STRICT = 'strict',
  
  /** Allow but warn - for development */
  WARN = 'warn',
  
  /** Disabled - for legacy code */
  OFF = 'off'
}

/**
 * TypeScript quality requirements
 */
export interface TypeScriptQualityConfig {
  /** Prohibit 'any' types completely */
  noAny: boolean;
  
  /** Require strict TypeScript compilation */
  strict: boolean;
  
  /** Require explicit return types for functions */
  explicitReturnTypes?: boolean;
  
  /** Require proper null checking */
  strictNullChecks?: boolean;
}

/**
 * ESLint quality requirements
 */
export interface ESLintQualityConfig {
  /** Whether to enforce ESLint rules */
  enforce: boolean;
  
  /** Enforcement level for ESLint issues */
  level: ESLintLevel;
  
  /** Custom ESLint configuration file (optional) */
  configFile?: string;
  
  /** Additional ESLint rules to enforce */
  additionalRules?: Record<string, any>;
}

/**
 * Test requirements
 */
export interface TestQualityConfig {
  /** Whether tests are required */
  required: boolean;
  
  /** Minimum test coverage percentage (0-100) */
  coverage?: number;
  
  /** Required test types */
  types?: ('unit' | 'integration' | 'e2e')[];
  
  /** Whether tests must pass for graduation */
  mustPass?: boolean;
}

/**
 * Module compliance requirements
 */
export interface ComplianceQualityConfig {
  /** Whether module compliance checking is required */
  required: boolean;
  
  /** Minimum compliance score (0-100) */
  minimumScore?: number;
  
  /** Required module structure elements */
  structure?: {
    packageJson: boolean;
    readme: boolean;
    mainFile: boolean;
    testDirectory: boolean;
    typeDefinitions?: boolean;
  };
}

/**
 * Documentation requirements
 */
export interface DocumentationQualityConfig {
  /** Whether documentation is required */
  required: boolean;
  
  /** Required documentation files */
  files?: ('README.md' | 'CHANGELOG.md' | 'API.md')[];
  
  /** Whether API documentation is required */
  apiDocs?: boolean;
  
  /** Whether examples are required */
  examples?: boolean;
}

/**
 * Security requirements
 */
export interface SecurityQualityConfig {
  /** Whether security audit is required */
  auditRequired: boolean;
  
  /** Whether to scan for vulnerabilities */
  vulnerabilityScan?: boolean;
  
  /** Whether to check for secrets in code */
  secretsScan?: boolean;
  
  /** Maximum allowed vulnerability severity */
  maxSeverity?: 'low' | 'medium' | 'high' | 'critical';
}

/**
 * Performance requirements
 */
export interface PerformanceQualityConfig {
  /** Whether performance testing is required */
  required: boolean;
  
  /** Maximum build time in milliseconds */
  maxBuildTime?: number;
  
  /** Maximum bundle size in bytes */
  maxBundleSize?: number;
  
  /** Whether memory leak detection is required */
  memoryLeakDetection?: boolean;
}

/**
 * Automated quality verification - SYSTEM MANAGED, READ-ONLY
 */
export interface QualityVerification {
  /** Last verification timestamp */
  lastChecked: string;
  
  /** Verification result by quality enforcement system */
  verifiedStatus: ModuleGraduationStatus;
  
  /** Quality score (0-100) calculated automatically */
  qualityScore: number;
  
  /** Issues detected by system */
  issues: {
    eslint: number;
    typescript: number;
    tests: number;
    compliance: number;
  };
  
  /** System-detected quality degradation reasons */
  degradationReasons?: string[];
  
  /** Cannot be manually edited - managed by QualityEnforcementEngine */
  readonly systemManaged: true;
}

/**
 * Complete module quality configuration
 */
export interface ModuleQualityConfig {
  /** Module graduation status - TARGET STATUS (what we want) */
  status: ModuleGraduationStatus;
  
  /** AUTOMATED VERIFICATION - actual current status */
  verification?: QualityVerification;
  
  /** ESLint configuration */
  eslint?: ESLintQualityConfig;
  
  /** TypeScript configuration */
  typescript?: TypeScriptQualityConfig;
  
  /** Test configuration */
  tests?: TestQualityConfig;
  
  /** Module compliance configuration */
  compliance?: ComplianceQualityConfig;
  
  /** Documentation configuration */
  documentation?: DocumentationQualityConfig;
  
  /** Security configuration */
  security?: SecurityQualityConfig;
  
  /** Performance configuration */
  performance?: PerformanceQualityConfig;
  
  /** Custom quality checks */
  custom?: Record<string, any>;
  
  /** Override enforcement mode for this module */
  enforcementOverride?: {
    commit?: 'strict' | 'warn' | 'off';
    push?: 'strict' | 'warn' | 'off';
  };
}

/**
 * Package.json continuum configuration with quality
 */
export interface ContinuumPackageConfig {
  /** Module type */
  type: 'daemon' | 'widget' | 'command' | 'integration' | 'utility';
  
  /** Module category */
  category?: string;
  
  /** Quality configuration */
  quality?: ModuleQualityConfig;
  
  /** Other continuum-specific config */
  [key: string]: any;
}

/**
 * Complete package.json structure with continuum config
 */
export interface ModulePackageJson {
  name: string;
  version: string;
  description?: string;
  main?: string;
  type?: 'module' | 'commonjs';
  
  /** Continuum-specific configuration */
  continuum?: ContinuumPackageConfig;
  
  /** Standard package.json fields */
  [key: string]: any;
}

/**
 * Default quality configurations for different graduation statuses
 */
export const DEFAULT_QUALITY_CONFIGS: Record<ModuleGraduationStatus, ModuleQualityConfig> = {
  graduated: {
    status: 'graduated',
    eslint: {
      enforce: true,
      level: 'strict'
    },
    typescript: {
      noAny: true,
      strict: true,
      explicitReturnTypes: true,
      strictNullChecks: true
    },
    tests: {
      required: true,
      coverage: 90,
      types: ['unit', 'integration'],
      mustPass: true
    },
    compliance: {
      required: true,
      minimumScore: 100,
      structure: {
        packageJson: true,
        readme: true,
        mainFile: true,
        testDirectory: true,
        typeDefinitions: true
      }
    },
    documentation: {
      required: true,
      files: ['README.md'],
      apiDocs: true
    },
    security: {
      auditRequired: true,
      vulnerabilityScan: true,
      secretsScan: true,
      maxSeverity: 'low'
    }
  },
  
  candidate: {
    status: 'candidate',
    eslint: {
      enforce: true,
      level: 'warn'
    },
    typescript: {
      noAny: false,
      strict: true
    },
    tests: {
      required: true,
      coverage: 70,
      types: ['unit']
    },
    compliance: {
      required: true,
      minimumScore: 70
    }
  },
  
  whitelisted: {
    status: 'whitelisted',
    eslint: {
      enforce: false,
      level: 'off'
    },
    typescript: {
      noAny: false,
      strict: false
    },
    tests: {
      required: false
    },
    compliance: {
      required: false
    }
  }
};

/**
 * Type guard to check if an object is a valid ModuleQualityConfig
 */
export function isValidModuleQualityConfig(obj: any): obj is ModuleQualityConfig {
  return obj && 
         typeof obj === 'object' && 
         typeof obj.status === 'string' &&
         ['graduated', 'candidate', 'whitelisted'].includes(obj.status);
}

/**
 * Merge quality config with defaults based on graduation status
 */
export function mergeWithDefaults(
  config: Partial<ModuleQualityConfig>, 
  status: ModuleGraduationStatus
): ModuleQualityConfig {
  const defaults = DEFAULT_QUALITY_CONFIGS[status];
  
  return {
    ...defaults,
    ...config,
    status,
    eslint: { ...defaults.eslint, ...config.eslint },
    typescript: { ...defaults.typescript, ...config.typescript },
    tests: { ...defaults.tests, ...config.tests },
    compliance: { ...defaults.compliance, ...config.compliance },
    documentation: { ...defaults.documentation, ...config.documentation },
    security: { ...defaults.security, ...config.security },
    performance: { ...defaults.performance, ...config.performance }
  };
}