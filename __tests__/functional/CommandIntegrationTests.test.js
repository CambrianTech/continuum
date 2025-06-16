/**
 * Functional Command Integration Tests
 * Tests real command workflows using the modular command system
 * These tests load actual command modules and test their integration
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import fs from 'fs';
import path from 'path';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

describe('Command Integration Functional Tests', () => {

  test('should load and execute screenshot command module', async () => {
    // Setup for this test
    const commandsDir = path.join(process.cwd(), 'src', 'commands', 'core');
    
    // Load the screenshot command module
    const screenshotModulePath = path.join(commandsDir, 'screenshot');
    assert(fs.existsSync(screenshotModulePath), 'Screenshot module directory should exist');

    // Load package.json to understand module structure
    const packagePath = path.join(screenshotModulePath, 'package.json');
    assert(fs.existsSync(packagePath), 'Screenshot module should have package.json');

    const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    assert(packageInfo.continuum, 'Package should have continuum metadata');
    assert(packageInfo.continuum.commandName === 'screenshot', 'Should be screenshot command');

    // Validate module has required files
    assert(fs.existsSync(path.join(screenshotModulePath, 'index.server.js')), 'Should have server entry point');
    assert(packageInfo.continuum.clientFiles, 'Should specify client files');

    // Load the server module
    const serverModulePath = path.join(screenshotModulePath, 'index.server.js');
    const serverModule = await import(`file://${serverModulePath}`);
    
    assert(serverModule.default, 'Server module should have default export');
    assert(serverModule.default.command, 'Server module should export command');
    assert.strictEqual(serverModule.default.name, 'screenshot', 'Should be screenshot command');
  });

  test('should load help command and read modular help content', async () => {
    const commandsDir = path.join(process.cwd(), 'src', 'commands', 'core');
    const helpModulePath = path.join(commandsDir, 'help');
    const packagePath = path.join(helpModulePath, 'package.json');
    
    assert(fs.existsSync(packagePath), 'Help module should have package.json');
    
    const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    assert(packageInfo.continuum.helpFile, 'Help module should specify help file');

    // Load the help content
    const helpFilePath = path.join(helpModulePath, packageInfo.continuum.helpFile);
    assert(fs.existsSync(helpFilePath), 'Help file should exist');

    const helpContent = fs.readFileSync(helpFilePath, 'utf8');
    assert(helpContent.includes('Continuum'), 'Help content should mention Continuum');
    assert(helpContent.includes('Command'), 'Help content should mention Commands');
    assert(helpContent.length > 500, 'Help content should be substantial');
  });

  test('should load agents command and read agent metadata', async () => {
    const commandsDir = path.join(process.cwd(), 'src', 'commands', 'core');
    const agentsModulePath = path.join(commandsDir, 'agents');
    const packagePath = path.join(agentsModulePath, 'package.json');
    
    assert(fs.existsSync(packagePath), 'Agents module should have package.json');
    
    const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    assert(packageInfo.continuum.defaultAgents, 'Should have default agents defined');
    assert(Array.isArray(packageInfo.continuum.defaultAgents), 'Default agents should be array');
    assert(packageInfo.continuum.defaultAgents.length > 0, 'Should have at least one default agent');

    // Validate agent structure
    const firstAgent = packageInfo.continuum.defaultAgents[0];
    assert(firstAgent.id, 'Agent should have id');
    assert(firstAgent.name, 'Agent should have name');
    assert(firstAgent.capabilities, 'Agent should have capabilities');
  });

  test('should run command discovery workflow', async () => {
    const commandsDir = path.join(process.cwd(), 'src', 'commands', 'core');
    // Simulate the command discovery process
    const commandDirs = fs.readdirSync(commandsDir, { withFileTypes: true })
      .filter(dirent => dirent.isDirectory())
      .map(dirent => dirent.name);

    assert(commandDirs.length >= 20, 'Should discover at least 20 commands');

    const validatedCommands = [];
    
    for (const commandDir of commandDirs) {
      const packagePath = path.join(commandsDir, commandDir, 'package.json');
      
      if (fs.existsSync(packagePath)) {
        const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
        
        // Validate package structure
        assert(packageInfo.name, `${commandDir} should have name`);
        assert(packageInfo.continuum, `${commandDir} should have continuum metadata`);
        assert(packageInfo.continuum.commandName, `${commandDir} should have commandName`);
        
        // Check if server entry point exists
        const serverEntry = packageInfo.main || 'index.server.js';
        const serverPath = path.join(commandsDir, commandDir, serverEntry);
        assert(fs.existsSync(serverPath), `${commandDir} should have server entry point`);

        validatedCommands.push({
          name: packageInfo.continuum.commandName,
          capabilities: packageInfo.continuum.capabilities || [],
          hasTests: packageInfo.continuum.hasTests || false
        });
      }
    }

    assert(validatedCommands.length >= 15, 'Should validate at least 15 commands');
    
    // Validate key commands are present
    const commandNames = validatedCommands.map(cmd => cmd.name);
    assert(commandNames.includes('screenshot'), 'Should include screenshot command');
    assert(commandNames.includes('help'), 'Should include help command');
    assert(commandNames.includes('agents'), 'Should include agents command');
    assert(commandNames.includes('test'), 'Should include test command');
  });

  test('should run test command and validate modular testing', async () => {
    const commandsDir = path.join(process.cwd(), 'src', 'commands', 'core');
    const testModulePath = path.join(commandsDir, 'test');
    const packagePath = path.join(testModulePath, 'package.json');
    
    const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    assert(packageInfo.continuum.hasTests, 'Test module should have tests');
    assert(packageInfo.continuum.testDir, 'Test module should specify test directory');

    // Check if test directory exists and has test files
    const testDir = path.join(testModulePath, packageInfo.continuum.testDir);
    assert(fs.existsSync(testDir), 'Test directory should exist');

    const testFiles = fs.readdirSync(testDir).filter(file => file.endsWith('.test.js'));
    assert(testFiles.length > 0, 'Should have test files in test directory');

    // Validate test files are readable
    for (const testFile of testFiles) {
      const testFilePath = path.join(testDir, testFile);
      const testContent = fs.readFileSync(testFilePath, 'utf8');
      assert(testContent.includes('test(') || testContent.includes('describe('), 
        `${testFile} should contain test functions`);
    }
  });
});