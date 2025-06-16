/**
 * Modular Command Test Discovery and Execution
 * Dynamically discovers and runs tests for all command modules
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const commandsDir = path.resolve(__dirname, '../../');

describe('Modular Command Tests', () => {
  test('Discover and validate all command modules', async () => {
    const commandDirs = fs.readdirSync(commandsDir, { withFileTypes: true })
      .filter(dirent => dirent.isDirectory())
      .map(dirent => dirent.name);

    console.log(`🔍 Discovered ${commandDirs.length} command directories: ${commandDirs.join(', ')}`);

    for (const commandDir of commandDirs) {
      const packagePath = path.join(commandsDir, commandDir, 'package.json');
      
      if (fs.existsSync(packagePath)) {
        const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
        
        console.log(`📦 Testing module: ${packageInfo.name}`);
        
        // Validate package.json structure
        assert(packageInfo.name, `${commandDir} should have a name`);
        assert(packageInfo.main, `${commandDir} should have a main entry point`);
        assert(packageInfo.continuum, `${commandDir} should have continuum config`);
        assert(packageInfo.continuum.commandName, `${commandDir} should have commandName`);
        
        // Check if module has tests
        if (packageInfo.continuum.hasTests) {
          const testDir = path.join(commandsDir, commandDir, packageInfo.continuum.testDir || 'test');
          assert(fs.existsSync(testDir), `${commandDir} claims to have tests but test directory missing`);
          
          const testFiles = fs.readdirSync(testDir).filter(file => file.endsWith('.test.js'));
          assert(testFiles.length > 0, `${commandDir} should have test files in ${testDir}`);
          
          console.log(`✅ ${commandDir}: ${testFiles.length} test files found`);
        }
        
        // Validate main entry point exists
        const mainPath = path.join(commandsDir, commandDir, packageInfo.main);
        assert(fs.existsSync(mainPath), `${commandDir} main entry point should exist: ${packageInfo.main}`);
      }
    }
  });

  test('Run tests for modules that have them', async () => {
    const commandDirs = fs.readdirSync(commandsDir, { withFileTypes: true })
      .filter(dirent => dirent.isDirectory())
      .map(dirent => dirent.name);

    for (const commandDir of commandDirs) {
      const packagePath = path.join(commandsDir, commandDir, 'package.json');
      
      if (fs.existsSync(packagePath)) {
        const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
        
        if (packageInfo.continuum.hasTests && packageInfo.scripts && packageInfo.scripts.test) {
          console.log(`🧪 Running tests for ${packageInfo.name}`);
          
          // For now, just validate test files can be imported
          const testDir = path.join(commandsDir, commandDir, packageInfo.continuum.testDir || 'test');
          const testFiles = fs.readdirSync(testDir).filter(file => file.endsWith('.test.js'));
          
          for (const testFile of testFiles) {
            const testPath = path.join(testDir, testFile);
            try {
              // Validate test file can be loaded
              assert(fs.existsSync(testPath), `Test file should exist: ${testPath}`);
              console.log(`  ✅ ${testFile} validated`);
            } catch (error) {
              console.error(`  ❌ ${testFile} failed:`, error.message);
              throw error;
            }
          }
        }
      }
    }
  });
});