#!/usr/bin/env npx tsx
/**
 * Registry-Aware Dynamic Port Cleanup
 * 
 * Uses ProcessRegistry to intelligently clean up JTAG processes:
 * - Identifies JTAG processes vs other applications
 * - Preserves active JTAG systems during startup
 * - Provides surgical cleanup for P2P mesh testing
 * - Prevents race conditions during system initialization
 */

// Process registry functionality now handled via modular command system
import { ProcessRegistryServerCommand } from '../commands/process-registry/server/ProcessRegistryServerCommand';
import { syncRegistryState } from '../system/core/registry/RegistrySync';
import { generateUUID } from '../system/core/types/CrossPlatformUUID';

class DynamicPortCleanup {
  /**
   * Registry-aware cleanup using modular process-registry command
   */
  public static async cleanup(forceAll = false): Promise<void> {
    try {
      // CRITICAL: Initialize SecretManager to load config.env into process.env
      // ServerConfig needs HTTP_PORT and WS_PORT from process.env
      const { SecretManager } = await import('../system/secrets/SecretManager');
      await SecretManager.getInstance().initialize();

      // CRITICAL: Synchronize registry state across all locations before cleanup
      // This solves the distributed state consistency problem
      console.log('🔄 Synchronizing registry state across all locations...');
      await syncRegistryState();
      console.log('✅ Registry state synchronized');

      // Create a mock command daemon for the server command
      const mockCommander = {
        subpath: 'system-cleanup',
        router: null as any,
        commands: new Map()
      };

      // Create proper JTAG context for the command
      const { createJTAGConfig } = await import('../system/shared/BrowserSafeConfig');
      const { createServerContext } = await import('../system/core/context/SecureJTAGContext');

      const jtagConfig = createJTAGConfig();
      const context = createServerContext(jtagConfig, 'cleanup-script');

      // Create the process registry server command
      const processRegistryCommand = new ProcessRegistryServerCommand(context, 'process-registry', mockCommander);

      // Execute cleanup through the modular command
      const result = await processRegistryCommand.cleanupProcesses({
        context,
        sessionId: generateUUID(), // Use proper UUID instead of hardcoded string
        forceAll,
        preserveActive: !forceAll
      });

      if (!result.success && result.errors.length > 0) {
        console.error('❌ Some cleanup operations failed:');
        result.errors.forEach(error => console.error(`  ${error}`));
        process.exit(1);
      }

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Registry-aware cleanup failed:', errorMsg);
      process.exit(1);
    }
  }
}

// Run cleanup if called directly
if (require.main === module) {
  // Check if this is a force cleanup (for system:stop)
  const forceCleanup = process.argv.includes('--force') || process.env.npm_lifecycle_event === 'system:stop';
  
  DynamicPortCleanup.cleanup(forceCleanup).catch((error) => {
    console.error('🚨 Cleanup script failed:', error);
    process.exit(1);
  });
}

export { DynamicPortCleanup };