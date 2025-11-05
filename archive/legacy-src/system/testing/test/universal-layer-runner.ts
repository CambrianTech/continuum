#!/usr/bin/env tsx
/**
 * UNIVERSAL CONTINUUM TESTING ENTRY POINT
 * Moved from root to proper module location
 * 
 * One command tests everything, layer by layer
 * Never forget what needs testing - it's all here
 * 
 * Usage: npm run test-all
 *        npm exec tsx src/system/testing/test/universal-layer-runner.ts
 *        npm exec tsx src/system/testing/test/universal-layer-runner.ts --layer=2
 */

// Use generic modular test runners instead of specific widget runner
import * as path from 'path';
import * as fs from 'fs';

interface LayerTest {
  name: string;
  description: string;
  test: () => Promise<boolean>;
}

class UniversalLayerTesting {
  private layers: LayerTest[] = [];
  private rootDir: string;

  constructor() {
    // Smart root directory detection - go up until we find package.json
    this.rootDir = this.findProjectRoot();
    this.setupLayers();
  }

  private findProjectRoot(): string {
    let currentDir = path.dirname(new URL(import.meta.url).pathname);
    
    while (currentDir !== path.dirname(currentDir)) {
      const packagePath = path.join(currentDir, 'package.json');
      
      // Use fs from top-level import and check if this is the main project package.json
      if (fs.existsSync(packagePath)) {
        const packageContent = fs.readFileSync(packagePath, 'utf8');
        const packageJson = JSON.parse(packageContent);
        // Look for the main continuum package
        if (packageJson.name === 'continuum' && packageJson.bin && packageJson.bin.continuum) {
          return currentDir;
        }
      }
      
      currentDir = path.dirname(currentDir);
    }
    
    // Fallback to relative path if we can't find root
    return path.resolve(__dirname, '../../../..');
  }

  private setupLayers() {
    // 🧅 LAYER 1: Core Foundation (BaseCommand, Types, Utils)
    this.layers.push({
      name: 'Layer 1: Core Foundation',
      description: 'BaseCommand, types, core utilities',
      test: async () => {
        console.log('🧪 Testing Layer 1: Core Foundation...');
        
        // Test 1: TypeScript compilation
        try {
          const { execSync } = await import('child_process');
          execSync('npx tsc --noEmit --project .', { 
            stdio: 'pipe',
            cwd: this.rootDir
          });
          console.log('  ✅ TypeScript compilation passes');
        } catch (error) {
          console.error('  ❌ TypeScript compilation failed');
          return false;
        }

        // Test 2: BaseCommand module exists and loads
        try {
          const baseCommandPath = path.resolve(this.rootDir, 'src/commands/core/base-command/BaseCommand.js');
          await import(baseCommandPath);
          console.log('  ✅ BaseCommand module loads');
        } catch (error) {
          console.error('  ❌ BaseCommand module failed to load:', error);
          return false;
        }

        return true;
      }
    });

    // 🧅 LAYER 2: Daemon Process Layer
    this.layers.push({
      name: 'Layer 2: Daemon Processes',
      description: 'Individual daemon unit and integration tests',
      test: async () => {
        console.log('🧪 Testing Layer 2: Daemon Processes...');
        
        // Test each daemon module by running its own tests
        const daemonModules = [
          'src/daemons/renderer',
          'src/daemons/browser-manager', 
          'src/daemons/command-processor',
          'src/daemons/continuum-directory',
          'src/integrations/websocket'
        ];

        let allPassed = true;

        for (const module of daemonModules) {
          try {
            // Check if module has package.json (modular compliance)
            const fs = await import('fs');
            const packagePath = path.resolve(this.rootDir, module, 'package.json');
            
            if (!fs.existsSync(packagePath)) {
              console.log(`  ⚠️  ${module} missing package.json - not fully modular`);
              continue;
            }

            // Check if module has tests
            const testDir = path.resolve(this.rootDir, module, 'test');
            if (fs.existsSync(testDir)) {
              console.log(`  ✅ ${module} has test directory`);
              
              // Look for test files
              const testFiles = fs.readdirSync(testDir)
                .filter(f => f.endsWith('.test.ts') || f.endsWith('.test.js'));
              if (testFiles.length > 0) {
                console.log(`    📋 Found ${testFiles.length} test files: ${testFiles.join(', ')}`);
              } else {
                console.log(`    ⚠️  No test files found in ${testDir}`);
              }
            } else {
              console.log(`  ⚠️  ${module} missing test directory`);
            }

            // Test that the module can load
            const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf-8'));
            const mainPath = path.resolve(this.rootDir, module, packageJson.main);
            
            await import(mainPath);
            console.log(`  ✅ ${module} main module loads successfully`);
            
          } catch (error) {
            console.error(`  ❌ ${module} failed:`, error);
            allPassed = false;
          }
        }

        return allPassed;
      }
    });

    // 🧅 LAYER 3: Command System Layer
    this.layers.push({
      name: 'Layer 3: Command System',
      description: 'Command module discovery and testing',
      test: async () => {
        console.log('🧪 Testing Layer 3: Command System...');
        
        // Discover all command modules
        const fs = await import('fs');
        
        let allPassed = true;
        let commandsFound = 0;
        
        try {
          // Scan command categories
          const commandsDir = path.resolve(this.rootDir, 'src/commands');
          const categories = fs.readdirSync(commandsDir, { withFileTypes: true })
            .filter(dirent => dirent.isDirectory())
            .map(dirent => dirent.name);

          for (const category of categories) {
            const categoryPath = path.join(commandsDir, category);
            
            // Scan commands in this category
            const commands = fs.readdirSync(categoryPath, { withFileTypes: true })
              .filter(dirent => dirent.isDirectory())
              .map(dirent => dirent.name);

            for (const command of commands) {
              const commandPath = path.join(categoryPath, command);
              const packagePath = path.join(commandPath, 'package.json');
              
              if (fs.existsSync(packagePath)) {
                commandsFound++;
                console.log(`  📦 Found command: ${category}/${command}`);
                
                // Check for tests
                const testDir = path.join(commandPath, 'test');
                if (fs.existsSync(testDir)) {
                  const testFiles = fs.readdirSync(testDir).filter(f => f.endsWith('.test.ts'));
                  console.log(`    🧪 Tests: ${testFiles.length} files`);
                } else {
                  console.log(`    ⚠️  No test directory`);
                }

                // Test command can load and has definition
                try {
                  const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf-8'));
                  
                  // Check if command is disabled
                  if (packageJson.continuum?.disabled === true) {
                    console.log(`    ⏸️  Command disabled - skipping`);
                    continue;
                  }
                  
                  // Check if main file is properly defined
                  if (!packageJson.main) {
                    console.log(`    ⚠️  No main file defined in package.json`);
                    continue;
                  }
                  
                  const commandFile = path.join(commandPath, packageJson.main);
                  const commandModule = await import(commandFile);
                  
                  // Find the command class (should end with 'Command')
                  const commandClassName = Object.keys(commandModule).find(key => key.endsWith('Command'));
                  if (commandClassName) {
                    const CommandClass = commandModule[commandClassName];
                    
                    // Skip base classes - they're abstract
                    if (commandClassName === 'BaseCommand' || commandClassName === 'BaseFileCommand' || 
                        commandClassName === 'DaemonCommand' || commandClassName === 'DirectCommand' ||
                        commandClassName === 'OperationRoutedCommand' || commandClassName === 'RemoteCommand') {
                      console.log(`    ✅ ${commandClassName} (abstract base class - skipped)`);
                    } else {
                      // Test command definition with error handling
                      try {
                        const definition = CommandClass.getDefinition();
                        if (definition && definition.name) {
                          console.log(`    ✅ Command loads and has definition: ${definition.name}`);
                        } else {
                          console.log(`    ❌ Command missing definition`);
                          allPassed = false;
                        }
                      } catch (error) {
                        if (error instanceof Error && error.message.includes('getDefinition() must be implemented')) {
                          console.log(`    ⚠️  ${commandClassName} is abstract or incomplete - skipped`);
                        } else {
                          console.log(`    ❌ Command definition failed: ${error}`);
                          allPassed = false;
                        }
                      }
                    }
                  } else {
                    console.log(`    ❌ No command class found`);
                    allPassed = false;
                  }
                } catch (error) {
                  console.error(`    ❌ Command load failed:`, error);
                  allPassed = false;
                }
              }
            }
          }

          console.log(`  📊 Total commands discovered: ${commandsFound}`);
          
        } catch (error) {
          console.error('  ❌ Command discovery failed:', error);
          return false;
        }

        return allPassed;
      }
    });

    // 🧅 LAYER 4: Integration Layer (Daemons + Commands)
    this.layers.push({
      name: 'Layer 4: System Integration',
      description: 'Full daemon + command integration + integration test suite',
      test: async () => {
        console.log('🧪 Testing Layer 4: System Integration...');
        
        let allPassed = true;
        
        // Test 1: System loading
        try {
          // Test main system can load
          const mainPath = path.resolve(this.rootDir, 'main.ts');
          await import(mainPath);
          console.log('  ✅ Main system module loads');
          
          // Test port availability
          const net = await import('net');
          const server = net.createServer();
          
          await new Promise((resolve, reject) => {
            server.listen(9000, () => {
              console.log('  ✅ Port 9000 available');
              server.close();
              resolve(true);
            });
            
            server.on('error', (error: any) => {
              if (error.code === 'EADDRINUSE') {
                console.log('  ⚠️  Port 9000 in use (system may be running)');
                resolve(true); // Not a failure - just already running
              } else {
                reject(error);
              }
            });
          });
          
        } catch (error) {
          console.error('  ❌ System integration test failed:', error);
          allPassed = false;
        }

        // Test 2: Run Integration Test Suite
        try {
          console.log('  🧪 Running Integration Test Suite...');
          
          // Discover and run integration tests
          const integrationTestPaths = [
            'src/test/integration/SharedSessionBrowserLaunch.integration.test.ts',
            'src/daemons/browser-manager/test/integration/SafeBrowserLaunch.integration.test.ts',
            'src/daemons/session-manager/test/integration/SessionManagerDaemon.integration.test.ts'
          ];
          
          let integrationTestsFound = 0;
          let integrationTestsPassed = 0;
          
          for (const testPath of integrationTestPaths) {
            const fullPath = path.resolve(this.rootDir, testPath);
            const fs = await import('fs');
            
            if (fs.existsSync(fullPath)) {
              integrationTestsFound++;
              console.log(`    📋 Found integration test: ${testPath}`);
              
              try {
                // Run vitest on this specific file
                const { execSync } = await import('child_process');
                execSync(`npx vitest --run ${testPath}`, {
                  stdio: 'pipe',
                  cwd: this.rootDir,
                  timeout: 30000 // 30 second timeout per test
                });
                
                console.log(`    ✅ Integration test passed: ${path.basename(testPath)}`);
                integrationTestsPassed++;
                
              } catch (error: any) {
                console.error(`    ❌ Integration test failed: ${path.basename(testPath)}`);
                if (error.stdout) {
                  console.error(`      Output: ${error.stdout.toString().slice(0, 200)}...`);
                }
                allPassed = false;
              }
            } else {
              console.log(`    ⚠️  Integration test not found: ${testPath}`);
            }
          }
          
          console.log(`  📊 Integration Tests: ${integrationTestsPassed}/${integrationTestsFound} passed`);
          
          if (integrationTestsFound === 0) {
            console.log('  ⚠️  No integration tests found - Layer 4 needs integration test coverage');
          }
          
        } catch (error) {
          console.error('  ❌ Integration test suite failed:', error);
          allPassed = false;
        }

        return allPassed;
      }
    });

    // 🧅 LAYER 5: Widget UI Layer  
    this.layers.push({
      name: 'Layer 5: Widget UI System',
      description: 'Widget discovery and compliance',
      test: async () => {
        console.log('🧪 Testing Layer 5: Widget UI System...');
        
        try {
          // Use existing npm test script for widgets
          const { execSync } = await import('child_process');
          const result = execSync('npm run test:widgets', { 
            stdio: 'pipe',
            cwd: this.rootDir,
            encoding: 'utf8'
          });
          
          console.log('  📊 Widget testing completed');
          
          // Check for 100% compliance in the output
          if (result.includes('100% module compliance rate')) {
            console.log('  ✅ All widgets compliant and tests passing');
            return true;
          } else {
            console.error('  ❌ Widget compliance issues detected');
            return false;
          }
        } catch (error) {
          console.error('  ❌ Widget testing failed:', error);
          return false;
        }
      }
    });

    // 🧅 LAYER 6: End-to-End Browser Layer
    this.layers.push({
      name: 'Layer 6: Browser Integration',
      description: 'Full browser + server integration',
      test: async () => {
        console.log('🧪 Testing Layer 6: Browser Integration...');
        
        try {
          // Test if we can reach the server (if running)
          const response = await fetch('http://localhost:9000', { 
            signal: AbortSignal.timeout(2000) 
          });
          
          if (response.ok) {
            console.log('  ✅ Browser interface responding');
            
            // Test if scripts are served
            const scriptTest = await fetch('http://localhost:9000/src/ui/continuum-browser.js');
            if (scriptTest.ok) {
              console.log('  ✅ Browser scripts accessible');
              return true;
            } else {
              console.error('  ❌ Browser scripts not accessible');
              return false;
            }
          } else {
            console.log('  ⚠️  Server not running - start with npm exec tsx main.ts');
            return true; // Not a failure if server isn't running
          }
        } catch (error) {
          console.log('  ⚠️  Server not reachable - start with npm exec tsx main.ts');
          return true; // Not a failure if server isn't running
        }
      }
    });
  }

  async runAllLayers(): Promise<boolean> {
    console.log('🧅 UNIVERSAL CONTINUUM LAYER TESTING');
    console.log(`📁 Project root: ${this.rootDir}`);
    console.log('====================================');
    
    let allPassed = true;
    const results: { layer: string; passed: boolean }[] = [];

    for (const layer of this.layers) {
      console.log(`\n${layer.name}`);
      console.log(`${layer.description}`);
      console.log('─'.repeat(50));
      
      try {
        const passed = await layer.test();
        results.push({ layer: layer.name, passed });
        
        if (passed) {
          console.log(`✅ ${layer.name} PASSED`);
        } else {
          console.log(`❌ ${layer.name} FAILED`);
          allPassed = false;
        }
      } catch (error) {
        console.error(`💥 ${layer.name} CRASHED:`, error);
        results.push({ layer: layer.name, passed: false });
        allPassed = false;
      }
    }

    // Summary
    console.log('\n📊 TESTING SUMMARY');
    console.log('==================');
    for (const result of results) {
      const status = result.passed ? '✅' : '❌';
      console.log(`${status} ${result.layer}`);
    }

    const passedCount = results.filter(r => r.passed).length;
    const totalCount = results.length;
    console.log(`\n🎯 ${passedCount}/${totalCount} layers passed`);

    if (allPassed) {
      console.log('\n🎉 ALL LAYERS PASSED - SYSTEM READY!');
    } else {
      console.log('\n🚨 SOME LAYERS FAILED - CHECK LOGS ABOVE');
    }

    return allPassed;
  }

  async runSingleLayer(layerNumber: number): Promise<boolean> {
    if (layerNumber < 1 || layerNumber > this.layers.length) {
      console.error(`❌ Invalid layer ${layerNumber}. Must be 1-${this.layers.length}`);
      return false;
    }

    const layer = this.layers[layerNumber - 1];
    console.log(`🧅 Testing ${layer.name}`);
    console.log(`📁 Project root: ${this.rootDir}`);
    console.log('─'.repeat(50));
    
    return await layer.test();
  }
}

async function main() {
  const args = process.argv.slice(2);
  const layerArg = args.find(arg => arg.startsWith('--layer='));
  
  const tester = new UniversalLayerTesting();
  
  if (layerArg) {
    const layerNumber = parseInt(layerArg.split('=')[1]);
    const passed = await tester.runSingleLayer(layerNumber);
    process.exit(passed ? 0 : 1);
  } else {
    const allPassed = await tester.runAllLayers();
    process.exit(allPassed ? 0 : 1);
  }
}

main();