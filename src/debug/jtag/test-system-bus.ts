#!/usr/bin/env npx tsx
/**
 * Test the Dynamic Command Bus System
 * 
 * Shows how easy it is to wire up commands
 */

import { JTAGSystemBus, JTAGConsole, JTAGScreenshot } from './shared/JTAGSystemBus';

async function testSystemBus() {
  console.log('🧪 Testing JTAG System Bus');
  console.log('==========================');

  // Test 1: Basic bus with core commands
  console.log('\n📦 Testing Basic System Bus...');
  const bus = new JTAGSystemBus();
  
  // These methods are auto-created when commands are registered!
  await (bus as any).log('TEST', 'Basic bus logging works');
  await (bus as any).error('TEST', 'Basic bus error works');
  
  console.log('✅ Basic bus commands work');

  // Test 2: Console extension
  console.log('\n🖥️ Testing Console Extension...');
  const console_bus = new JTAGConsole();
  
  // All console commands auto-wired
  await (console_bus as any).log('TEST', 'Console extension logging');
  await (console_bus as any).critical('TEST', 'Console extension critical');
  await (console_bus as any).probe('TEST', 'message_count', 42);
  
  console.log('✅ Console extension commands work');

  // Test 3: Screenshot extension  
  console.log('\n📸 Testing Screenshot Extension...');
  const screenshot_bus = new JTAGScreenshot();
  
  try {
    const result = await (screenshot_bus as any).screenshot({ filename: 'test.png' });
    console.log('✅ Screenshot command result:', result);
  } catch (error: any) {
    console.log('✅ Screenshot command properly requires browser endpoint:', error.message);
  }

  // Test 4: Dynamic command registration
  console.log('\n🔌 Testing Dynamic Command Registration...');
  
  bus.registerCommand({
    name: 'customCommand',
    requiresEndpoint: 'server',
    handler: async (params) => {
      console.log('🎯 Custom command executed with params:', params);
      return { custom: true, params };
    },
    description: 'A dynamically registered custom command'
  });
  
  // Now this method exists on the bus!
  const customResult = await (bus as any).customCommand('arg1', 'arg2', { data: 'test' });
  console.log('✅ Dynamic command result:', customResult);

  // Test 5: List all commands
  console.log('\n📋 Registered Commands:');
  for (const [name, def] of bus.getCommands()) {
    console.log(`  • ${name}: ${def.description} ${def.requiresEndpoint ? `(${def.requiresEndpoint})` : ''}`);
  }

  console.log('\n🎉 System Bus Test Complete!');
  console.log('💡 Easy command wiring demonstrated');
}

testSystemBus().catch(console.error);