#!/usr/bin/env tsx
/**
 * UNIVERSAL CONTINUUM LAUNCHER
 * 
 * One command to launch anything - never forget how to start
 * 
 * Usage: npm run launch
 *        npm exec tsx launch.ts
 *        npm exec tsx launch.ts --mode=test
 *        npm exec tsx launch.ts --mode=dev
 */

interface LaunchMode {
  name: string;
  description: string;
  command: string;
  args?: string[];
}

class UniversalLauncher {
  private modes: LaunchMode[] = [
    {
      name: 'system',
      description: 'Start full Continuum system (default)',
      command: 'tsx',
      args: ['main.ts']
    },
    {
      name: 'test',
      description: 'Run all layer tests',
      command: 'tsx', 
      args: ['test-all-layers.ts']
    },
    {
      name: 'test-widgets',
      description: 'Test only widget discovery and compliance',
      command: 'tsx',
      args: ['test-widgets.ts']
    },
    {
      name: 'compile',
      description: 'Check TypeScript compilation only',
      command: 'npx',
      args: ['tsc', '--noEmit', '--project', '.']
    },
    {
      name: 'dev',
      description: 'Development mode with file watching',
      command: 'tsx',
      args: ['--watch', 'main.ts']
    }
  ];

  showHelp() {
    console.log('🚀 UNIVERSAL CONTINUUM LAUNCHER');
    console.log('===============================');
    console.log('');
    console.log('Available modes:');
    console.log('');
    
    for (const mode of this.modes) {
      console.log(`  ${mode.name.padEnd(12)} - ${mode.description}`);
    }
    
    console.log('');
    console.log('Usage:');
    console.log('  npm exec tsx launch.ts                    # Start system (default)');
    console.log('  npm exec tsx launch.ts --mode=test        # Run tests');
    console.log('  npm exec tsx launch.ts --mode=dev         # Development mode');
    console.log('  npm exec tsx launch.ts --help             # Show this help');
    console.log('');
  }

  async launch(modeName: string = 'system'): Promise<number> {
    const mode = this.modes.find(m => m.name === modeName);
    
    if (!mode) {
      console.error(`❌ Unknown mode: ${modeName}`);
      console.error(`Available modes: ${this.modes.map(m => m.name).join(', ')}`);
      return 1;
    }

    console.log(`🚀 Launching Continuum in ${mode.name} mode`);
    console.log(`📝 ${mode.description}`);
    console.log('─'.repeat(50));

    try {
      const { spawn } = await import('child_process');
      
      const process = spawn(mode.command, mode.args || [], {
        stdio: 'inherit',
        shell: true
      });

      return new Promise((resolve) => {
        process.on('close', (code) => {
          console.log(`\n🏁 ${mode.name} mode exited with code ${code}`);
          resolve(code || 0);
        });

        process.on('error', (error) => {
          console.error(`💥 Launch failed:`, error);
          resolve(1);
        });

        // Handle Ctrl+C gracefully
        process.on('SIGINT', () => {
          console.log('\n🛑 Shutting down...');
          process.kill();
        });
      });
    } catch (error) {
      console.error(`💥 Failed to launch ${mode.name} mode:`, error);
      return 1;
    }
  }
}

async function main() {
  const args = process.argv.slice(2);
  
  if (args.includes('--help') || args.includes('-h')) {
    new UniversalLauncher().showHelp();
    return;
  }

  const modeArg = args.find(arg => arg.startsWith('--mode='));
  const mode = modeArg ? modeArg.split('=')[1] : 'system';
  
  const launcher = new UniversalLauncher();
  const exitCode = await launcher.launch(mode);
  process.exit(exitCode);
}

main();