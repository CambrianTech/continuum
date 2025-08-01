/**
 * JTAG Auto-Discovery Demo
 * 
 * This example demonstrates the breakthrough architecture:
 * - Build-time manifest generation
 * - Constructor dependency injection
 * - Universal discovery pattern
 * - Zero registration boilerplate
 */

import { JTAGSystem } from '../system/core/system/shared/JTAGSystem';

async function demonstrateAutoDiscovery() {
  console.log('🚀 JTAG Auto-Discovery Demo');
  console.log('============================');
  
  // Single line initialization - everything auto-discovered!
  console.log('1. Connecting to JTAG system...');
  const jtag = await JTAGSystem.connect();
  
  console.log('✅ Connected! System auto-discovered:');
  console.log(`   Environment: ${jtag.getSystemInfo().context.environment}`);
  console.log(`   Daemons: ${jtag.getSystemInfo().daemons.join(', ')}`);
  
  // Show what was auto-discovered
  const commandDaemon = jtag.getDaemons().get('CommandDaemon');
  if (commandDaemon) {
    const availableCommands = commandDaemon.getAvailableCommands();
    console.log(`   Commands: ${availableCommands.join(', ')}`);
  }
  
  console.log('\n2. Testing auto-discovered commands...');
  
  // Use auto-discovered screenshot command
  try {
    const screenshot = await jtag.commands.screenshot({
      filename: 'auto-discovery-demo.png',
      selector: 'body'
    });
    
    if (screenshot.success) {
      console.log(`✅ Screenshot captured: ${screenshot.filepath}`);
    } else {
      console.log(`⚠️ Screenshot failed: ${screenshot.error}`);
    }
  } catch (error: any) {
    console.log(`❌ Screenshot error: ${error.message}`);
  }
  
  console.log('\n3. System Information:');
  const systemInfo = jtag.getSystemInfo();
  console.log(JSON.stringify(systemInfo, null, 2));
  
  console.log('\n🎉 Demo complete! The entire system was auto-discovered:');
  console.log('   • No manual daemon registration');
  console.log('   • No manual command registration');  
  console.log('   • No configuration required');
  console.log('   • Same code works in browser and server');
  console.log('   • All dependencies injected via constructors');
  
  await jtag.shutdown();
}

// Run the demo
if (require.main === module) {
  demonstrateAutoDiscovery().catch(console.error);
}

export { demonstrateAutoDiscovery };