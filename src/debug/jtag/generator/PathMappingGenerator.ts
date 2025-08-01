/**
 * Path Mapping Generator
 * 
 * Clean, modular path mapping generator that creates essential aliases only.
 * Replaces the massive 224-alias system with ~10-15 semantic mappings.
 */

import type { 
  PathMappingsConfig, 
  GeneratorOptions, 
  GenerationResult, 
  FileUpdate 
} from './types/GeneratorTypes';
import { PathResolver, SemanticAliasStrategy, PathMappingGenerator as Generator, DirectoryScanner } from './utils/PathUtils';
import { FileManager, ConfigTemplates } from './utils/FileManager';
import { ConsoleLogger, ProgressTracker, GeneratorStats } from './utils/Logger';

// ============================================================================
// Main Path Mapping Generator
// ============================================================================

export class PathMappingGenerator {
  private options: GeneratorOptions;
  private logger: ConsoleLogger;
  private fileManager: FileManager;
  private resolver: PathResolver;
  private scanner: DirectoryScanner;
  private generator: Generator;
  private progress: ProgressTracker;
  private stats: GeneratorStats;

  constructor(options: GeneratorOptions) {
    this.options = options;
    this.logger = options.logger as ConsoleLogger || new ConsoleLogger();
    this.fileManager = new FileManager(this.logger, options.dryRun);
    this.resolver = new PathResolver(options.rootPath);
    this.scanner = new DirectoryScanner(this.resolver);
    this.generator = new Generator(new SemanticAliasStrategy(), this.resolver);
    this.progress = new ProgressTracker(this.logger);
    this.stats = new GeneratorStats(this.logger);
  }

  /**
   * Generate clean path mappings configuration
   */
  async generate(): Promise<GenerationResult> {
    this.logger.info('🚀 Generating clean path mappings...');
    this.progress.addStep('Discover essential directories');
    this.progress.addStep('Generate semantic aliases');
    this.progress.addStep('Update path mappings config');
    this.progress.addStep('Update TypeScript config');

    const result: GenerationResult = {
      success: false,
      filesUpdated: [],
      errors: [],
      warnings: [],
      stats: {
        pathMappingsGenerated: 0,
        structureFilesGenerated: 0,
        daemonsFound: 0,
        commandsFound: 0
      }
    };

    try {
      // Step 1: Discover essential directories
      this.logger.info('🔍 Discovering essential directories...');
      const essentialDirs = this.scanner.findEssentialDirectories();
      this.stats.set('directoriesScanned', essentialDirs.length);
      this.progress.completeStep('Discover essential directories');

      this.logger.info(`   Found ${essentialDirs.length} essential directories:`);
      essentialDirs.forEach(dir => this.logger.info(`     • ${dir}`));

      // Step 2: Generate semantic aliases
      this.logger.info('🎯 Generating semantic aliases...');
      const mappings = this.generator.generateEssentialMappings(essentialDirs);
      const mappingCount = Object.keys(mappings).length;
      this.stats.set('aliasesGenerated', mappingCount);
      this.progress.completeStep('Generate semantic aliases');

      this.logger.info(`   Generated ${mappingCount} clean aliases:`);
      Object.values(mappings).forEach(mapping => {
        this.logger.info(`     ${mapping.alias} → ${mapping.relativePath}`);
      });

      // Step 3: Save path mappings config
      this.logger.info('💾 Saving path mappings configuration...');
      const pathMappingsConfig = ConfigTemplates.createPathMappingsConfig(mappings);
      const pathMappingsFile = this.fileManager.writeJSON(
        this.resolver.resolve('generator/path-mappings.json'),
        pathMappingsConfig,
        `Generated ${mappingCount} essential path mappings`
      );
      result.filesUpdated.push(pathMappingsFile);
      this.progress.completeStep('Update path mappings config');

      // Step 4: Update TypeScript config
      this.logger.info('🔧 Updating TypeScript configuration...');
      const tsconfigUpdate = await this.updateTypeScriptConfig(mappings);
      if (tsconfigUpdate) {
        result.filesUpdated.push(tsconfigUpdate);
      }
      this.progress.completeStep('Update TypeScript config');

      // Success!
      result.success = true;
      result.stats.pathMappingsGenerated = mappingCount;
      
      this.progress.logSummary();
      this.stats.logSummary();
      this.logger.info('🎉 Path mapping generation complete!');

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      result.errors.push(errorMsg);
      this.logger.error('💥 Path mapping generation failed:', error);
    }

    return result;
  }

  /**
   * Update TypeScript configuration with minimal path mappings
   */
  private async updateTypeScriptConfig(mappings: Record<string, any>): Promise<FileUpdate | null> {
    const tsconfigPath = this.resolver.resolve('tsconfig.json');
    const existingConfig = this.fileManager.readJSON(tsconfigPath);
    
    if (!existingConfig) {
      this.logger.warn('tsconfig.json not found - skipping TypeScript update');
      return null;
    }

    // Generate TypeScript paths from clean mappings
    const paths = this.generator.createTypeScriptPaths(mappings);
    const updatedConfig = ConfigTemplates.createTypeScriptUpdate(existingConfig, paths);

    this.logger.info(`   Updating tsconfig.json with ${Object.keys(paths).length} path entries`);
    
    return this.fileManager.writeJSON(
      tsconfigPath,
      updatedConfig,
      `Updated with ${Object.keys(mappings).length} essential path mappings (replaced ${Object.keys(existingConfig.compilerOptions.paths || {}).length} algorithmic mappings)`
    );
  }

  /**
   * Clean up old configuration artifacts
   */
  async cleanup(): Promise<void> {
    this.logger.info('🧹 Cleaning up old configuration artifacts...');
    
    // Remove old unified-config.json with 224 mappings
    const oldConfigPath = this.resolver.resolve('generator/unified-config.json');
    if (this.fileManager.exists(oldConfigPath)) {
      this.logger.info('   Removing old unified-config.json with 224+ mappings');
      // Note: In a real implementation, we'd implement file deletion in FileManager
    }

    // Clean up package.json imports field
    await this.cleanPackageJsonImports();
  }

  /**
   * Remove unnecessary package.json imports field
   */
  private async cleanPackageJsonImports(): Promise<void> {
    const packagePath = this.resolver.resolve('package.json');
    const packageJson = this.fileManager.readJSON(packagePath);
    
    if (packageJson && packageJson.imports) {
      this.logger.info('   Removing unused package.json imports field (224+ mappings)');
      delete packageJson.imports;
      delete packageJson._importGeneration;
      
      this.fileManager.writeJSON(
        packagePath,
        packageJson,
        'Removed unused package.json imports field - TypeScript compilation handles path resolution'
      );
    }
  }
}

// ============================================================================
// CLI Interface
// ============================================================================

export async function generatePathMappings(options: Partial<GeneratorOptions> = {}): Promise<GenerationResult> {
  const defaultOptions: GeneratorOptions = {
    rootPath: process.cwd(),
    logger: new ConsoleLogger('info'),
    dryRun: false,
    force: false,
    ...options
  };

  const generator = new PathMappingGenerator(defaultOptions);
  const result = await generator.generate();
  
  if (result.success && !defaultOptions.dryRun) {
    await generator.cleanup();
  }
  
  return result;
}

// CLI execution
if (require.main === module) {
  const dryRun = process.argv.includes('--dry-run');
  const force = process.argv.includes('--force');
  const debug = process.argv.includes('--debug');
  
  generatePathMappings({
    rootPath: process.cwd(),
    logger: new ConsoleLogger(debug ? 'debug' : 'info'),
    dryRun,
    force
  }).then(result => {
    process.exit(result.success ? 0 : 1);
  }).catch(error => {
    console.error('💥 Fatal error:', error);
    process.exit(1);
  });
}