#!/usr/bin/env tsx
/**
 * Unified System Startup - Single entry point for all system operations
 * 
 * This replaces all the scattered startup scripts with a single, robust
 * entry point that handles different scenarios through command line flags.
 * 
 * Usage:
 *   npx tsx scripts/unified-startup.ts --mode=development
 *   npx tsx scripts/unified-startup.ts --mode=testing
 *   npx tsx scripts/unified-startup.ts --mode=validation
 */

import { SystemOrchestration } from '../system/core/SystemOrchestrator';

interface StartupArgs {
  mode: 'development' | 'testing' | 'validation' | 'cli';
  help?: boolean;
}

function parseArgs(): StartupArgs {
  const args = process.argv.slice(2);
  
  if (args.includes('--help') || args.includes('-h')) {
    return { help: true, mode: 'development' };
  }
  
  const modeArg = args.find(arg => arg.startsWith('--mode='));
  const mode = modeArg ? modeArg.split('=')[1] as StartupArgs['mode'] : 'development';
  
  if (!['development', 'testing', 'validation', 'cli'].includes(mode)) {
    throw new Error(`Invalid mode: ${mode}`);
  }
  
  return { mode };
}

function showHelp(): void {
  console.log(`
🎯 JTAG Unified System Startup

Usage:
  npx tsx scripts/unified-startup.ts [options]

Options:
  --mode=development    Simple development startup (default)
                       • No tmux persistence
                       • Show output + capture logs
                       • Build if needed
                       
  --mode=testing       Testing mode with background system
                       • Tmux persistence for background tests
                       • Log output only (clean test output)
                       • Build if needed
                       
  --mode=validation    Fast validation for git hooks
                       • Tmux persistence
                       • Log output only
                       • Build if needed
                       
  --mode=cli          Adaptive for CLI commands
                       • Check existing system first
                       • Start if needed
                       • Show output to user

Examples:
  npm start                           # Development mode
  npm test                            # Testing mode  
  git hooks                           # Validation mode
  ./continuum [command]              # CLI mode

🎯 This unified approach replaces:
  • scripts/system-startup.ts
  • scripts/launch-and-capture.ts  
  • scripts/test-with-server.ts
  • Multiple npm script entry points
`);
}

async function main(): Promise<void> {
  try {
    const args = parseArgs();
    
    if (args.help) {
      showHelp();
      return;
    }
    
    console.log(`🎯 JTAG Unified Startup - Mode: ${args.mode}`);
    console.log('📋 Single entry point for all system operations\n');
    
    let result;
    
    switch (args.mode) {
      case 'development':
        result = await SystemOrchestration.forDevelopment();
        break;
      case 'testing':
        result = await SystemOrchestration.forTesting();
        break;
      case 'validation':
        result = await SystemOrchestration.forValidation();
        break;
      case 'cli':
        result = await SystemOrchestration.forCLI();
        break;
    }
    
    if (result.success) {
      console.log('✅ System ready!');
      console.log(`📊 Status: ${result.state.health}`);
      console.log(`🌐 Ports: ${result.state.ports.join(', ')}`);
      if (result.pid) {
        console.log(`🎯 PID: ${result.pid}`);
      }
      if (result.logFile) {
        console.log(`📄 Logs: ${result.logFile}`);
      }
      
      // For development mode, system should stay running but script exits
      if (args.mode === 'development') {
        console.log('🚀 System running - ready for development');
      }
      
      process.exit(0);
    } else {
      console.error('❌ System startup failed');
      if (result.errorMessage) {
        console.error(`💥 Error: ${result.errorMessage}`);
      }
      console.error(`📊 Status: ${result.state.health}`);
      process.exit(1);
    }
    
  } catch (error) {
    console.error('💥 Unified startup failed:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

// Only run if called directly (not imported)
if (require.main === module) {
  main();
}

export { main as unifiedStartup };