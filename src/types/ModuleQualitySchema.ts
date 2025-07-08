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
  /** PERFECT - Highest achievable quality, exemplary module, battle-tested */
  PERFECT = 'perfect',
  
  /** GRADUATED - Zero tolerance enforcement, production ready */
  GRADUATED = 'graduated',
  
  /** CANDIDATE - All quality standards met, ready for graduation */
  CANDIDATE = 'candidate', 
  
  /** WHITELISTED - Work in progress, development flexibility */
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
  perfect: {
    status: ModuleGraduationStatus.PERFECT,
    eslint: {
      enforce: true,
      level: ESLintLevel.STRICT
    },
    typescript: {
      noAny: true,
      strict: true,
      explicitReturnTypes: true,
      strictNullChecks: true
    },
    tests: {
      required: true,
      coverage: 100,
      types: ['unit', 'integration', 'e2e'],
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
      files: ['README.md', 'CHANGELOG.md', 'API.md'],
      apiDocs: true,
      examples: true
    },
    security: {
      auditRequired: true,
      vulnerabilityScan: true,
      secretsScan: true,
      maxSeverity: 'low'
    },
    performance: {
      required: true,
      maxBuildTime: 5000,
      maxBundleSize: 100000,
      memoryLeakDetection: true
    }
  },
  graduated: {
    status: ModuleGraduationStatus.GRADUATED,
    eslint: {
      enforce: true,
      level: ESLintLevel.STRICT
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
    status: ModuleGraduationStatus.CANDIDATE,
    eslint: {
      enforce: true,
      level: ESLintLevel.WARN
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
    status: ModuleGraduationStatus.WHITELISTED,
    eslint: {
      enforce: false,
      level: ESLintLevel.OFF
    },
    typescript: {
      noAny: false,
      strict: false
    },
    tests: {
      required: false,
      coverage: 0,
      types: [],
      mustPass: false
    },
    compliance: {
      required: false,
      minimumScore: 0,
      structure: {
        packageJson: false,
        readme: false,
        mainFile: false,
        testDirectory: false,
        typeDefinitions: false
      }
    },
    documentation: {
      required: false,
      files: [],
      apiDocs: false,
      examples: false
    },
    security: {
      auditRequired: false,
      vulnerabilityScan: false,
      secretsScan: false,
      maxSeverity: 'critical' as const
    },
    performance: {
      required: false,
      maxBuildTime: Infinity,
      maxBundleSize: Infinity,
      memoryLeakDetection: false
    }
  },
  
  degraded: {
    status: ModuleGraduationStatus.DEGRADED,
    eslint: {
      enforce: false,
      level: ESLintLevel.OFF
    },
    typescript: {
      noAny: false,
      strict: false
    },
    tests: {
      required: false,
      coverage: 0,
      types: [],
      mustPass: false
    },
    compliance: {
      required: false,
      minimumScore: 0,
      structure: {
        packageJson: false,
        readme: false,
        mainFile: false,
        testDirectory: false,
        typeDefinitions: false
      }
    },
    documentation: {
      required: false,
      files: [],
      apiDocs: false,
      examples: false
    },
    security: {
      auditRequired: false,
      vulnerabilityScan: false,
      secretsScan: false,
      maxSeverity: 'critical' as const
    },
    performance: {
      required: false,
      maxBuildTime: Infinity,
      maxBundleSize: Infinity,
      memoryLeakDetection: false
    }
  },
  
  broken: {
    status: ModuleGraduationStatus.BROKEN,
    eslint: {
      enforce: false,
      level: ESLintLevel.OFF
    },
    typescript: {
      noAny: false,
      strict: false
    },
    tests: {
      required: false,
      coverage: 0,
      types: [],
      mustPass: false
    },
    compliance: {
      required: false,
      minimumScore: 0,
      structure: {
        packageJson: false,
        readme: false,
        mainFile: false,
        testDirectory: false,
        typeDefinitions: false
      }
    },
    documentation: {
      required: false,
      files: [],
      apiDocs: false,
      examples: false
    },
    security: {
      auditRequired: false,
      vulnerabilityScan: false,
      secretsScan: false,
      maxSeverity: 'critical' as const
    },
    performance: {
      required: false,
      maxBuildTime: Infinity,
      maxBundleSize: Infinity,
      memoryLeakDetection: false
    }
  },
  
  unknown: {
    status: ModuleGraduationStatus.UNKNOWN,
    eslint: {
      enforce: false,
      level: ESLintLevel.OFF
    },
    typescript: {
      noAny: false,
      strict: false
    },
    tests: {
      required: false,
      coverage: 0,
      types: [],
      mustPass: false
    },
    compliance: {
      required: false,
      minimumScore: 0,
      structure: {
        packageJson: false,
        readme: false,
        mainFile: false,
        testDirectory: false,
        typeDefinitions: false
      }
    },
    documentation: {
      required: false,
      files: [],
      apiDocs: false,
      examples: false
    },
    security: {
      auditRequired: false,
      vulnerabilityScan: false,
      secretsScan: false,
      maxSeverity: 'critical' as const
    },
    performance: {
      required: false,
      maxBuildTime: Infinity,
      maxBundleSize: Infinity,
      memoryLeakDetection: false
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
  if (!defaults) {
    throw new Error(`Invalid module graduation status: ${status}`);
  }
  
  return {
    status,
    eslint: {
      enforce: config.eslint?.enforce ?? defaults.eslint.enforce,
      level: config.eslint?.level ?? defaults.eslint.level,
      ...(config.eslint?.configFile && { configFile: config.eslint.configFile }),
      ...(config.eslint?.additionalRules && { additionalRules: config.eslint.additionalRules })
    },
    typescript: {
      noAny: config.typescript?.noAny ?? defaults.typescript.noAny,
      strict: config.typescript?.strict ?? defaults.typescript.strict,
      ...(config.typescript?.explicitReturnTypes !== undefined && { explicitReturnTypes: config.typescript.explicitReturnTypes }),
      ...(config.typescript?.strictNullChecks !== undefined && { strictNullChecks: config.typescript.strictNullChecks })
    },
    tests: {
      required: config.tests?.required ?? defaults.tests.required,
      coverage: config.tests?.coverage ?? defaults.tests.coverage,
      types: config.tests?.types ?? defaults.tests.types,
      mustPass: config.tests?.mustPass ?? defaults.tests.mustPass
    },
    compliance: {
      required: config.compliance?.required ?? defaults.compliance.required,
      minimumScore: config.compliance?.minimumScore ?? defaults.compliance.minimumScore,
      structure: config.compliance?.structure ?? defaults.compliance.structure
    },
    documentation: {
      required: config.documentation?.required ?? defaults.documentation.required,
      files: config.documentation?.files ?? defaults.documentation.files,
      apiDocs: config.documentation?.apiDocs ?? defaults.documentation.apiDocs,
      examples: config.documentation?.examples ?? defaults.documentation.examples
    },
    security: {
      auditRequired: config.security?.auditRequired ?? defaults.security.auditRequired,
      vulnerabilityScan: config.security?.vulnerabilityScan ?? defaults.security.vulnerabilityScan,
      secretsScan: config.security?.secretsScan ?? defaults.security.secretsScan,
      maxSeverity: config.security?.maxSeverity ?? defaults.security.maxSeverity
    },
    performance: {
      required: config.performance?.required ?? defaults.performance.required,
      maxBuildTime: config.performance?.maxBuildTime ?? defaults.performance.maxBuildTime,
      maxBundleSize: config.performance?.maxBundleSize ?? defaults.performance.maxBundleSize,
      memoryLeakDetection: config.performance?.memoryLeakDetection ?? defaults.performance.memoryLeakDetection
    }
  };
}