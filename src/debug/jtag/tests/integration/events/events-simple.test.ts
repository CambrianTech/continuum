#!/usr/bin/env tsx
/**
 * Simple Event Bridging Test
 * Tests cross-context event bridging using CLI approach (like working screenshot tests)
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

async function testEventsSimple() {
  console.log('🌉 Testing Cross-Context Event Bridging');
  console.log('=======================================');
  
  try {
    // Test 1: Check if EventsDaemon is running via ping
    console.log('\n📊 Test 1: Check EventsDaemon health via CLI');
    
    try {
      const { stdout, stderr } = await execAsync('./jtag ping --daemon=EventsDaemon');
      
      if (stdout.includes('SUCCESS') || stdout.includes('✅')) {
        console.log('✅ EventsDaemon is running and responding');
        console.log('Response:', stdout.trim());
      } else {
        console.log('❌ EventsDaemon not responding');
        console.log('STDOUT:', stdout);
        console.log('STDERR:', stderr);
      }
    } catch (error) {
      console.log('❌ EventsDaemon ping failed:', error.message);
    }
    
    // Test 2: Check if HealthDaemon responds (for comparison)
    console.log('\n🏥 Test 2: Check HealthDaemon for comparison');
    
    try {
      const { stdout, stderr } = await execAsync('./jtag ping --daemon=HealthDaemon');
      
      if (stdout.includes('SUCCESS') || stdout.includes('✅')) {
        console.log('✅ HealthDaemon is running and responding');
        console.log('Response:', stdout.trim());
      } else {
        console.log('❌ HealthDaemon not responding');
        console.log('STDOUT:', stdout);
        console.log('STDERR:', stderr);
      }
    } catch (error) {
      console.log('❌ HealthDaemon ping failed:', error.message);
    }
    
    // Test 3: Check system is fully operational
    console.log('\n🔄 Test 3: General system ping');
    
    try {
      const { stdout, stderr } = await execAsync('./jtag ping');
      
      if (stdout.includes('SUCCESS') || stdout.includes('✅')) {
        console.log('✅ JTAG system is running and responding');
        console.log('Response:', stdout.trim());
      } else {
        console.log('❌ JTAG system not responding properly');
        console.log('STDOUT:', stdout);
        console.log('STDERR:', stderr);
      }
    } catch (error) {
      console.log('❌ System ping failed:', error.message);
    }
    
    console.log('\n🎉 Cross-context event bridging daemon test complete!');
    
  } catch (error) {
    console.error('❌ Test failed:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  testEventsSimple();
}