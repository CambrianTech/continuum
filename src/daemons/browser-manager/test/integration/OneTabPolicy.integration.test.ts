/**
 * ONE TAB POLICY Integration Tests
 * 
 * Tests that the browser manager ALWAYS maintains exactly ONE tab
 * regardless of the chaos thrown at it.
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

class OneTabPolicyTest {
  private baseUrl = 'http://localhost:9000';
  
  /**
   * Count how many tabs are open to localhost:9000
   */
  private async countTabs(): Promise<number> {
    try {
      // Count established connections
      const { stdout: connections } = await execAsync(
        `lsof -i :9000 | grep ESTABLISHED | wc -l`
      );
      return parseInt(connections.trim()) || 0;
    } catch (error) {
      return 0;
    }
  }
  
  /**
   * Open a new tab to localhost:9000
   */
  private async openTab(): Promise<void> {
    await execAsync(`open ${this.baseUrl}`);
    // Give it time to connect
    await this.delay(2000);
  }
  
  /**
   * Run ./continuum
   */
  private async runContinuum(): Promise<void> {
    await execAsync('./continuum');
    // Give it time to stabilize
    await this.delay(3000);
  }
  
  /**
   * Helper delay function
   */
  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
  
  /**
   * Test: Starting with 0 tabs
   */
  async testZeroTabs(): Promise<void> {
    console.log('\n🧪 TEST: Starting with 0 tabs');
    
    // Close all tabs first
    await execAsync('npx tsx force-one-tab.ts 2>/dev/null || true');
    await this.delay(2000);
    
    const before = await this.countTabs();
    console.log(`  📊 Before: ${before} tabs`);
    
    await this.runContinuum();
    
    const after = await this.countTabs();
    console.log(`  📊 After: ${after} tabs`);
    console.log(`  ${after === 1 ? '✅ PASS' : '❌ FAIL'}: Should have exactly 1 tab`);
  }
  
  /**
   * Test: Starting with 1 tab
   */
  async testOneTab(): Promise<void> {
    console.log('\n🧪 TEST: Starting with 1 tab');
    
    // Ensure exactly 1 tab
    await execAsync('npx tsx force-one-tab.ts 2>/dev/null || true');
    await this.openTab();
    await this.delay(2000);
    
    const before = await this.countTabs();
    console.log(`  📊 Before: ${before} tabs`);
    
    await this.runContinuum();
    
    const after = await this.countTabs();
    console.log(`  📊 After: ${after} tabs`);
    console.log(`  ${after === 1 ? '✅ PASS' : '❌ FAIL'}: Should maintain exactly 1 tab`);
  }
  
  /**
   * Test: Starting with 2 tabs
   */
  async testTwoTabs(): Promise<void> {
    console.log('\n🧪 TEST: Starting with 2 tabs');
    
    // Open 2 tabs
    await this.openTab();
    await this.openTab();
    await this.delay(2000);
    
    const before = await this.countTabs();
    console.log(`  📊 Before: ${before} tabs`);
    
    await this.runContinuum();
    
    const after = await this.countTabs();
    console.log(`  📊 After: ${after} tabs`);
    console.log(`  ${after === 1 ? '✅ PASS' : '❌ FAIL'}: Should consolidate to 1 tab`);
  }
  
  /**
   * Test: Chaos - many tabs
   */
  async testChaos(): Promise<void> {
    console.log('\n🧪 TEST: Chaos - opening many tabs');
    
    // Open many tabs
    for (let i = 0; i < 5; i++) {
      await this.openTab();
      await this.delay(500);
    }
    
    const before = await this.countTabs();
    console.log(`  📊 Before: ${before} tabs`);
    
    await this.runContinuum();
    
    const after = await this.countTabs();
    console.log(`  📊 After: ${after} tabs`);
    console.log(`  ${after === 1 ? '✅ PASS' : '❌ FAIL'}: Should consolidate to 1 tab`);
  }
  
  /**
   * Test: Multiple continuum calls
   */
  async testMultipleContinuumCalls(): Promise<void> {
    console.log('\n🧪 TEST: Multiple continuum calls');
    
    await this.openTab();
    const initial = await this.countTabs();
    console.log(`  📊 Initial: ${initial} tabs`);
    
    // Call continuum multiple times
    for (let i = 0; i < 3; i++) {
      console.log(`  🔄 Call ${i + 1}...`);
      await this.runContinuum();
      const count = await this.countTabs();
      console.log(`    📊 Tabs: ${count}`);
    }
    
    const final = await this.countTabs();
    console.log(`  📊 Final: ${final} tabs`);
    console.log(`  ${final === 1 ? '✅ PASS' : '❌ FAIL'}: Should maintain exactly 1 tab`);
  }
  
  /**
   * Run all tests
   */
  async runAll(): Promise<void> {
    console.log('🚀 ONE TAB POLICY INTEGRATION TESTS');
    console.log('===================================');
    console.log('Rule: Always maintain exactly ONE tab to localhost:9000');
    
    try {
      await this.testZeroTabs();
      await this.testOneTab();
      await this.testTwoTabs();
      await this.testChaos();
      await this.testMultipleContinuumCalls();
      
      console.log('\n✅ All tests completed!');
    } catch (error) {
      console.error('\n❌ Test suite failed:', error);
    }
  }
}

// Run the tests
const test = new OneTabPolicyTest();
test.runAll();

export { OneTabPolicyTest };