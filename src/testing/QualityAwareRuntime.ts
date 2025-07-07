/**
 * Quality-Aware Runtime System
 * 
 * Uses package.json quality verification metadata to make dynamic runtime decisions
 * about which modules to load, execute, and trust based on their verified quality status.
 */

import { ModuleGraduationStatus, ModulePackageJson, QualityVerification } from '../types/ModuleQualitySchema.js';
import { readdir, readFile } from 'fs/promises';
import { join } from 'path';

export interface QualityFilterOptions {
  minimumQuality: ModuleGraduationStatus;
  productionMode: boolean;
  allowDegraded: boolean;
  allowUnknown: boolean;
}

export interface ModuleWithQuality {
  name: string;
  path: string;
  packageJson: ModulePackageJson;
  verification?: QualityVerification;
  qualityRank: number;
}

export class QualityAwareRuntime {
  private readonly qualityRanks = new Map([
    [ModuleGraduationStatus.PERFECT, 100],
    [ModuleGraduationStatus.GRADUATED, 90],
    [ModuleGraduationStatus.CANDIDATE, 70],
    [ModuleGraduationStatus.WHITELISTED, 50],
    [ModuleGraduationStatus.DEGRADED, 20],
    [ModuleGraduationStatus.BROKEN, 0],
    [ModuleGraduationStatus.UNKNOWN, 10]
  ]);

  /**
   * Discover modules with quality filtering
   */
  async discoverQualityModules(
    basePath: string, 
    moduleType: 'daemon' | 'widget' | 'command' | 'integration',
    options: Partial<QualityFilterOptions> = {}
  ): Promise<ModuleWithQuality[]> {
    const defaults: QualityFilterOptions = {
      minimumQuality: ModuleGraduationStatus.CANDIDATE,
      productionMode: false,
      allowDegraded: false,
      allowUnknown: true
    };
    
    const opts = { ...defaults, ...options };
    const modules: ModuleWithQuality[] = [];
    
    try {
      const entries = await readdir(basePath, { withFileTypes: true });
      
      for (const entry of entries) {
        if (entry.isDirectory()) {
          const modulePath = join(basePath, entry.name);
          const packageJsonPath = join(modulePath, 'package.json');
          
          try {
            const packageContent = await readFile(packageJsonPath, 'utf8');
            const packageJson: ModulePackageJson = JSON.parse(packageContent);
            
            // Filter by module type
            if (packageJson.continuum?.type !== moduleType) {
              continue;
            }
            
            const verification = packageJson.continuum?.quality?.verification;
            const verifiedStatus = verification?.verifiedStatus || ModuleGraduationStatus.UNKNOWN;
            const qualityRank = this.getQualityRank(verifiedStatus);
            
            // Apply quality filters
            if (!this.passesQualityFilter(verifiedStatus, opts)) {
              console.log(`⚠️ Skipping ${entry.name}: Quality ${verifiedStatus} filtered out`);
              continue;
            }
            
            modules.push({
              name: entry.name,
              path: modulePath,
              packageJson,
              verification,
              qualityRank
            });
            
          } catch (error) {
            console.log(`❌ Failed to read package.json for ${entry.name}:`, error instanceof Error ? error.message : String(error));
          }
        }
      }
      
      // Sort by quality rank (highest first)
      return modules.sort((a, b) => b.qualityRank - a.qualityRank);
      
    } catch (error) {
      console.error(`Failed to discover modules in ${basePath}:`, error instanceof Error ? error.message : String(error));
      return [];
    }
  }

  /**
   * Get quality rank for comparison
   */
  getQualityRank(status: ModuleGraduationStatus): number {
    return this.qualityRanks.get(status) || 0;
  }

  /**
   * Check if module passes quality filter
   */
  private passesQualityFilter(status: ModuleGraduationStatus, options: QualityFilterOptions): boolean {
    // Always block broken modules
    if (status === ModuleGraduationStatus.BROKEN) {
      return false;
    }
    
    // Block degraded unless explicitly allowed
    if (status === ModuleGraduationStatus.DEGRADED && !options.allowDegraded) {
      return false;
    }
    
    // Block unknown unless explicitly allowed
    if (status === ModuleGraduationStatus.UNKNOWN && !options.allowUnknown) {
      return false;
    }
    
    // Production mode requires graduated or perfect
    if (options.productionMode && 
        status !== ModuleGraduationStatus.GRADUATED && 
        status !== ModuleGraduationStatus.PERFECT) {
      return false;
    }
    
    // Check minimum quality requirement
    const qualityRank = this.getQualityRank(status);
    const minimumRank = this.getQualityRank(options.minimumQuality);
    
    return qualityRank >= minimumRank;
  }

  /**
   * Execute command with quality-aware behavior
   */
  async executeWithQualityContext(
    module: ModuleWithQuality,
    executeFn: () => Promise<any>
  ): Promise<any> {
    const status = module.verification?.verifiedStatus || ModuleGraduationStatus.UNKNOWN;
    
    switch (status) {
      case ModuleGraduationStatus.PERFECT:
      case ModuleGraduationStatus.GRADUATED:
        // Execute normally - high confidence
        console.log(`✅ Executing ${module.name} (${status})`);
        return await executeFn();
        
      case ModuleGraduationStatus.CANDIDATE:
        // Execute with monitoring
        console.log(`⚠️ Executing candidate-quality module ${module.name}`);
        return await this.executeWithMonitoring(module, executeFn);
        
      case ModuleGraduationStatus.WHITELISTED:
        // Execute in development mode only
        console.log(`🔧 Executing whitelisted module ${module.name} (development)`);
        return await this.executeWithWarnings(module, executeFn);
        
      case ModuleGraduationStatus.DEGRADED:
        // Execute with extra caution
        console.log(`🔧 Module ${module.name} degraded - executing with caution`);
        return await this.executeWithFallback(module, executeFn);
        
      case ModuleGraduationStatus.BROKEN:
        // Refuse to execute
        throw new Error(`Module ${module.name} is broken and cannot be executed`);
        
      default:
        // Unknown quality - development only with warnings
        console.log(`❓ Module ${module.name} has unknown quality - proceed with caution`);
        return await this.executeWithMaxWarnings(module, executeFn);
    }
  }

  /**
   * Execute with monitoring for candidate modules
   */
  private async executeWithMonitoring(
    module: ModuleWithQuality,
    executeFn: () => Promise<any>
  ): Promise<any> {
    const startTime = Date.now();
    try {
      const result = await executeFn();
      const duration = Date.now() - startTime;
      console.log(`📊 ${module.name} executed successfully in ${duration}ms`);
      return result;
    } catch (error) {
      console.error(`⚠️ Candidate module ${module.name} failed:`, error instanceof Error ? error.message : String(error));
      throw error;
    }
  }

  /**
   * Execute with warnings for whitelisted modules
   */
  private async executeWithWarnings(
    module: ModuleWithQuality,
    executeFn: () => Promise<any>
  ): Promise<any> {
    console.warn(`⚠️ WARNING: Executing whitelisted module ${module.name} - quality not verified`);
    return await executeFn();
  }

  /**
   * Execute with fallback for degraded modules
   */
  private async executeWithFallback(
    module: ModuleWithQuality,
    executeFn: () => Promise<any>
  ): Promise<any> {
    try {
      return await executeFn();
    } catch (error) {
      console.error(`🔧 Degraded module ${module.name} failed, using fallback behavior`);
      // Could implement actual fallback logic here
      throw new Error(`Module ${module.name} failed and no fallback available`);
    }
  }

  /**
   * Execute with maximum warnings for unknown quality
   */
  private async executeWithMaxWarnings(
    module: ModuleWithQuality,
    executeFn: () => Promise<any>
  ): Promise<any> {
    console.warn(`🚨 CAUTION: Executing module ${module.name} with unknown quality status`);
    console.warn(`🚨 This module has not been validated by the quality system`);
    return await executeFn();
  }

  /**
   * Get system quality health report
   */
  async getSystemQualityReport(basePath: string): Promise<{
    daemons: Record<string, number>;
    widgets: Record<string, number>;
    commands: Record<string, number>;
    overallHealth: number;
    recommendations: string[];
  }> {
    const [daemons, widgets, commands] = await Promise.all([
      this.discoverQualityModules(join(basePath, 'daemons'), 'daemon', { allowDegraded: true, allowUnknown: true }),
      this.discoverQualityModules(join(basePath, 'ui/components'), 'widget', { allowDegraded: true, allowUnknown: true }),
      this.discoverQualityModules(join(basePath, 'commands'), 'command', { allowDegraded: true, allowUnknown: true })
    ]);

    const statusCounts = (modules: ModuleWithQuality[]) => {
      const counts: Record<string, number> = {};
      for (const status of Object.values(ModuleGraduationStatus)) {
        counts[status] = modules.filter(m => 
          (m.verification?.verifiedStatus || ModuleGraduationStatus.UNKNOWN) === status
        ).length;
      }
      return counts;
    };

    const daemonCounts = statusCounts(daemons);
    const widgetCounts = statusCounts(widgets);
    const commandCounts = statusCounts(commands);

    const totalModules = daemons.length + widgets.length + commands.length;
    const qualityModules = totalModules - (
      daemonCounts[ModuleGraduationStatus.BROKEN] + 
      daemonCounts[ModuleGraduationStatus.DEGRADED] +
      widgetCounts[ModuleGraduationStatus.BROKEN] + 
      widgetCounts[ModuleGraduationStatus.DEGRADED] +
      commandCounts[ModuleGraduationStatus.BROKEN] + 
      commandCounts[ModuleGraduationStatus.DEGRADED]
    );

    const overallHealth = totalModules > 0 ? Math.round((qualityModules / totalModules) * 100) : 0;

    const recommendations: string[] = [];
    if (daemonCounts[ModuleGraduationStatus.BROKEN] > 0) {
      recommendations.push(`Fix ${daemonCounts[ModuleGraduationStatus.BROKEN]} broken daemons`);
    }
    if (daemonCounts[ModuleGraduationStatus.DEGRADED] > 0) {
      recommendations.push(`Address ${daemonCounts[ModuleGraduationStatus.DEGRADED]} degraded daemons`);
    }

    return {
      daemons: daemonCounts,
      widgets: widgetCounts,
      commands: commandCounts,
      overallHealth,
      recommendations
    };
  }
}