#!/usr/bin/env tsx
/**
 * Generate Command Tests Script
 * 
 * Automatically creates test files for all JTAG commands using the standard template.
 * Ensures consistent testing structure across all commands.
 */

import { promises as fs } from 'fs';
import { join, dirname } from 'path';

interface CommandInfo {
  name: string;
  path: string;
  hasSharedTypes: boolean;
  hasBrowserImplementation: boolean;
  hasServerImplementation: boolean;
}

/**
 * Discover all commands in the commands directory
 */
async function discoverCommands(): Promise<CommandInfo[]> {
  const commandsPath = join(__dirname, '..');
  const commands: CommandInfo[] = [];
  
  const entries = await fs.readdir(commandsPath, { withFileTypes: true });
  
  for (const entry of entries) {
    if (entry.isDirectory() && entry.name !== 'test') {
      const commandPath = join(commandsPath, entry.name);
      
      // Check for nested commands (like file/save, chat/send-message)
      if (await hasNestedCommands(commandPath)) {
        const nestedCommands = await discoverNestedCommands(commandPath);
        commands.push(...nestedCommands);
      } else {
        // Single command
        const commandInfo = await analyzeCommand(entry.name, commandPath);
        if (commandInfo) {
          commands.push(commandInfo);
        }
      }
    }
  }
  
  return commands;
}

async function hasNestedCommands(commandPath: string): Promise<boolean> {
  try {
    const entries = await fs.readdir(commandPath, { withFileTypes: true });
    const hasShared = entries.some(e => e.isDirectory() && e.name === 'shared');
    const hasSubCommands = entries.some(e => e.isDirectory() && e.name !== 'shared' && e.name !== 'browser' && e.name !== 'server');
    
    return hasSubCommands && !hasShared;
  } catch {
    return false;
  }
}

async function discoverNestedCommands(basePath: string): Promise<CommandInfo[]> {
  const commands: CommandInfo[] = [];
  const baseName = basePath.split('/').pop() || '';
  
  try {
    const entries = await fs.readdir(basePath, { withFileTypes: true });
    
    for (const entry of entries) {
      if (entry.isDirectory() && entry.name !== 'shared') {
        const subCommandPath = join(basePath, entry.name);
        const commandInfo = await analyzeCommand(`${baseName}/${entry.name}`, subCommandPath);
        if (commandInfo) {
          commands.push(commandInfo);
        }
      }
    }
  } catch (error) {
    console.warn(`Failed to discover nested commands in ${basePath}:`, error instanceof Error ? error.message : String(error));
  }
  
  return commands;
}

async function analyzeCommand(name: string, path: string): Promise<CommandInfo | null> {
  try {
    const entries = await fs.readdir(path, { withFileTypes: true });
    
    const hasShared = entries.some(e => e.isDirectory() && e.name === 'shared');
    const hasBrowser = entries.some(e => e.isDirectory() && e.name === 'browser');
    const hasServer = entries.some(e => e.isDirectory() && e.name === 'server');
    
    if (!hasShared) {
      return null; // Not a valid command structure
    }
    
    // Check for types file in shared
    const sharedPath = join(path, 'shared');
    const sharedEntries = await fs.readdir(sharedPath);
    const hasSharedTypes = sharedEntries.some(f => f.endsWith('Types.ts'));
    
    return {
      name,
      path,
      hasSharedTypes,
      hasBrowserImplementation: hasBrowser,
      hasServerImplementation: hasServer
    };
  } catch (error) {
    console.warn(`Failed to analyze command ${name}:`, error instanceof Error ? error.message : String(error));
    return null;
  }
}

/**
 * Generate test files for a command
 */
async function generateTestsForCommand(command: CommandInfo): Promise<void> {
  const testDir = join(command.path, 'test');
  const unitDir = join(testDir, 'unit');
  const integrationDir = join(testDir, 'integration');
  
  // Create test directories
  await fs.mkdir(testDir, { recursive: true });
  await fs.mkdir(unitDir, { recursive: true });
  await fs.mkdir(integrationDir, { recursive: true });
  
  // Generate command name variations
  const commandName = command.name.replace(/\//g, '');
  const commandNamePascal = toPascalCase(commandName);
  const commandNameLower = command.name.toLowerCase();
  
  // Load template
  const templatePath = join(__dirname, 'templates', 'CommandTestTemplate.ts');
  let template = await fs.readFile(templatePath, 'utf8');
  
  // Replace template placeholders
  template = template.replace(/\[COMMAND_NAME\]/g, commandNamePascal);
  template = template.replace(/\[COMMAND_NAME_LOWERCASE\]/g, commandNameLower);
  
  // Determine types import path
  let typesImport = `../shared/${commandNamePascal}Types`;
  if (command.hasSharedTypes) {
    const sharedFiles = await fs.readdir(join(command.path, 'shared'));
    const typesFile = sharedFiles.find(f => f.endsWith('Types.ts'));
    if (typesFile) {
      const typesName = typesFile.replace('.ts', '');
      typesImport = `../shared/${typesName}`;
    }
  }
  
  template = template.replace(/\.\.\/shared\/\[COMMAND_NAME\]Types/g, typesImport);
  
  // Create unit test file
  const unitTestPath = join(unitDir, `${commandNamePascal}Command.test.ts`);
  if (!await fileExists(unitTestPath)) {
    await fs.writeFile(unitTestPath, template);
    console.log(`✅ Created unit test: ${unitTestPath}`);
  } else {
    console.log(`⏭️  Unit test already exists: ${unitTestPath}`);
  }
  
  // Create integration test file
  const integrationTestPath = join(integrationDir, `${commandNamePascal}Integration.test.ts`);
  if (!await fileExists(integrationTestPath)) {
    // Modify template for integration focus
    const integrationTemplate = template
      .replace(/Unit Test:/g, 'Integration Test:')
      .replace(/🧪.*Command Tests/g, `🧪 ${commandNamePascal} Command Integration Tests`)
      .replace(/UNIT TESTS/g, 'INTEGRATION TESTS')
      .replace(/Starting.*Command Tests/g, `Starting ${commandNamePascal} Command Integration Tests`);
    
    await fs.writeFile(integrationTestPath, integrationTemplate);
    console.log(`✅ Created integration test: ${integrationTestPath}`);
  } else {
    console.log(`⏭️  Integration test already exists: ${integrationTestPath}`);
  }
}

/**
 * Helper functions
 */
function toPascalCase(str: string): string {
  return str
    .split(/[-_\/]/)
    .map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
    .join('');
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await fs.access(path);
    return true;
  } catch {
    return false;
  }
}

/**
 * Generate README for each command's test directory
 */
async function generateTestReadme(command: CommandInfo): Promise<void> {
  const testDir = join(command.path, 'test');
  const readmePath = join(testDir, 'README.md');
  
  if (await fileExists(readmePath)) {
    return; // Don't overwrite existing README
  }
  
  const commandNamePascal = toPascalCase(command.name.replace(/\//g, ''));
  
  const readmeContent = `# ${commandNamePascal} Command Tests

Tests for the ${command.name} command following middle-out testing methodology.

## Structure

\`\`\`
test/
├── unit/                     # Unit tests (isolated, mocked dependencies)
│   └── ${commandNamePascal}Command.test.ts
├── integration/              # Integration tests (real client connections)
│   └── ${commandNamePascal}Integration.test.ts  
└── README.md                # This file
\`\`\`

## Running Tests

\`\`\`bash
# Run unit tests only
npx tsx test/unit/${commandNamePascal}Command.test.ts

# Run integration tests only  
npx tsx test/integration/${commandNamePascal}Integration.test.ts

# Run all tests for this command
npm test -- --testPathPattern="${command.name}"
\`\`\`

## Test Coverage

### Unit Tests
- ✅ Basic command execution with mocked dependencies
- ✅ Error handling scenarios
- ✅ Performance validation
- ✅ Parameter validation

### Integration Tests  
- ✅ Real command execution through client
- ✅ Cross-environment testing (browser/server)
- ✅ End-to-end correlation and response handling
- ✅ Bootstrap session compatibility

## Command-Specific Notes

${command.hasBrowserImplementation ? '- Has browser implementation' : '- Server-only implementation'}
${command.hasServerImplementation ? '- Has server implementation' : '- Browser-only implementation'}
${command.hasSharedTypes ? '- Uses shared types for consistency' : '- May need custom type definitions'}

For reusable testing utilities, see \`commands/test/utils/\`.
`;

  await fs.writeFile(readmePath, readmeContent);
  console.log(`✅ Created test README: ${readmePath}`);
}

/**
 * Main execution
 */
async function main(): Promise<void> {
  try {
    console.log('🏗️ Generating command tests for all JTAG commands...\n');
    
    // Discover all commands
    const commands = await discoverCommands();
    console.log(`📋 Found ${commands.length} commands:\n`);
    
    commands.forEach(cmd => {
      const indicators = [
        cmd.hasBrowserImplementation ? 'B' : '-',
        cmd.hasServerImplementation ? 'S' : '-', 
        cmd.hasSharedTypes ? 'T' : '-'
      ].join('');
      console.log(`   ${cmd.name.padEnd(25)} [${indicators}]`);
    });
    
    console.log(`\n🚀 Generating test files...\n`);
    
    // Generate tests for each command
    let created = 0;
    let skipped = 0;
    
    for (const command of commands) {
      try {
        console.log(`\n📝 Processing ${command.name}...`);
        await generateTestsForCommand(command);
        await generateTestReadme(command);
        created++;
      } catch (error) {
        console.error(`❌ Failed to generate tests for ${command.name}:`, error instanceof Error ? error.message : String(error));
        skipped++;
      }
    }
    
    console.log(`\n🎉 Test generation complete!`);
    console.log(`📊 Summary:`);
    console.log(`   Commands processed: ${commands.length}`);
    console.log(`   Tests created: ${created}`);
    console.log(`   Skipped: ${skipped}`);
    
    console.log(`\n📋 Next steps:`);
    console.log(`   1. Customize generated test files with command-specific logic`);
    console.log(`   2. Update test parameters in each test file`);
    console.log(`   3. Run cross-environment tests: npx tsx commands/test/AllCommandsCrossEnvironment.test.ts`);
    console.log(`   4. Add command-specific assertions and validation`);
    
  } catch (error) {
    console.error('❌ Test generation failed:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main();
}