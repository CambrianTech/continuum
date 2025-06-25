#!/usr/bin/env npx tsx
/**
 * TypeScript Command Testing Script
 * Register and test TypeScript commands to verify migration success
 */

import { TypeScriptCommandRegistry } from './TypeScriptCommandRegistry';
import { EmotionCommand } from './ui/emotion/EmotionCommand';
import { InfoCommand } from './core/info/InfoCommand';
import { StatusTextCommand } from './core/status_text/StatusTextCommand';

async function testTypeScriptCommands() {
  console.log('🧪 Testing TypeScript Commands...\n');
  
  const registry = new TypeScriptCommandRegistry();
  
  // Register commands
  console.log('📚 Registering TypeScript Commands:');
  registry.registerCommand(EmotionCommand);
  registry.registerCommand(InfoCommand);
  registry.registerCommand(StatusTextCommand);
  
  console.log(`\n✅ Registered ${registry.getAllCommands().length} TypeScript commands\n`);
  
  // Test EmotionCommand
  console.log('🎭 Testing EmotionCommand:');
  try {
    const emotionResult = await registry.executeCommand('EMOTION', {
      feeling: 'joy',
      intensity: 'high',
      duration: 2000
    });
    console.log('✅ EmotionCommand:', emotionResult.success ? 'PASSED' : 'FAILED');
    if (!emotionResult.success) {
      console.log('❌ Error:', emotionResult.error);
    }
  } catch (error) {
    console.log('❌ EmotionCommand FAILED:', error.message);
  }
  
  // Test InfoCommand
  console.log('\n💻 Testing InfoCommand:');
  try {
    const infoResult = await registry.executeCommand('INFO', {
      section: 'system'
    });
    console.log('✅ InfoCommand:', infoResult.success ? 'PASSED' : 'FAILED');
    if (!infoResult.success) {
      console.log('❌ Error:', infoResult.error);
    }
  } catch (error) {
    console.log('❌ InfoCommand FAILED:', error.message);
  }
  
  // Test StatusTextCommand
  console.log('\n💬 Testing StatusTextCommand:');
  try {
    // Create mock context
    const mockContext = {
      continuonStatus: {
        updateStatusText: (text: string) => console.log(`Mock status update: ${text}`),
        getStatus: () => ({ status: 'connected', ready: true }),
        currentStatus: 'connected'
      }
    };
    
    const statusResult = await registry.executeCommand('STATUS_TEXT', {
      text: 'TypeScript test message',
      duration: 1000
    }, mockContext);
    console.log('✅ StatusTextCommand:', statusResult.success ? 'PASSED' : 'FAILED');
    if (!statusResult.success) {
      console.log('❌ Error:', statusResult.error);
    }
  } catch (error) {
    console.log('❌ StatusTextCommand FAILED:', error.message);
  }
  
  // Test command discovery
  console.log('\n📋 Command Registry Status:');
  const commands = registry.getAllCommands();
  const byCategory = commands.reduce((acc, cmd) => {
    acc[cmd.category] = acc[cmd.category] || [];
    acc[cmd.category].push(cmd.name);
    return acc;
  }, {} as { [key: string]: string[] });
  
  for (const [category, commandNames] of Object.entries(byCategory)) {
    console.log(`  ${category}: ${commandNames.join(', ')}`);
  }
  
  // Generate documentation
  console.log('\n📖 Generating Documentation:');
  const docs = registry.generateDocumentation();
  console.log('✅ Documentation generated:', docs.length, 'characters');
  
  // Show migration status
  console.log('\n📊 Migration Status:');
  const migrationStatus = registry.getMigrationStatus();
  console.log(`  Migrated Commands: ${migrationStatus.length}`);
  migrationStatus.forEach(status => {
    console.log(`  ✅ ${status.command} (${status.details.category})`);
  });
  
  console.log('\n🎯 TypeScript Command Testing Complete!');
  console.log('\n📈 Next Steps:');
  console.log('  1. Run unit tests: npm test');
  console.log('  2. Migrate more commands to TypeScript');
  console.log('  3. Integrate with legacy command system');
  console.log('  4. Update git hooks to run TypeScript tests');
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  testTypeScriptCommands().catch(error => {
    console.error('❌ TypeScript command testing failed:', error);
    process.exit(1);
  });
}

export { testTypeScriptCommands };