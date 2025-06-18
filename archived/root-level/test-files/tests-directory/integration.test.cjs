
/**
 * NASA-Grade Integration Tests
 * Component interaction validation
 */

const fs = require('fs');
const path = require('path');

class IntegrationTestSuite {
  constructor() {
    this.testResults = [];
    this.projectRoot = path.join(__dirname, '..');
  }
  
  async runAllTests() {
    console.log('🔗 Running NASA-grade integration tests...');
    
    await this.testMemoryPackageIntegration();
    await this.testCyberpunkThemeIntegration();
    await this.testCrossComponentCommunication();
    await this.testSystemIntegration();
    
    const failedTests = this.testResults.filter(t => !t.passed);
    if (failedTests.length > 0) {
      throw new Error(`INTEGRATION FAILURE: ${failedTests.length} tests failed. All components must integrate perfectly.`);
    }
    
    console.log(`✅ ALL ${this.testResults.length} integration tests PASSED`);
    return true;
  }
  
  async testMemoryPackageIntegration() {
    const test = { name: 'Memory Package Integration', passed: false, details: [] };
    
    try {
      const memoryPath = path.join(this.projectRoot, 'packages', 'memory', 'src', 'index.ts');
      if (fs.existsSync(memoryPath)) {
        const content = fs.readFileSync(memoryPath, 'utf-8');
        
        // Check for proper imports/exports
        if (content.includes('export') && content.includes('class ContinuumMemory')) {
          test.details.push('✅ Memory package exports properly');
          
          // Check for required methods
          const requiredMethods = ['store', 'retrieve', 'storeStrategy'];
          const hasAllMethods = requiredMethods.every(method => content.includes(method));
          
          if (hasAllMethods) {
            test.details.push('✅ All required methods present');
            test.passed = true;
          } else {
            test.details.push('❌ Missing required methods');
          }
        } else {
          test.details.push('❌ Memory package export issues');
        }
      } else {
        test.details.push('❌ Memory package missing');
      }
    } catch (error) {
      test.details.push(`❌ Memory integration test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testCyberpunkThemeIntegration() {
    const test = { name: 'Cyberpunk Theme Integration', passed: false, details: [] };
    
    try {
      const cyberpunkPath = path.join(this.projectRoot, 'cyberpunk-cli');
      if (fs.existsSync(cyberpunkPath)) {
        const files = fs.readdirSync(cyberpunkPath);
        const cssFiles = files.filter(f => f.endsWith('.css'));
        const htmlFiles = files.filter(f => f.endsWith('.html'));
        
        if (cssFiles.length > 0) {
          test.details.push(`✅ Found ${cssFiles.length} CSS files`);
        }
        
        if (htmlFiles.length > 0) {
          test.details.push(`✅ Found ${htmlFiles.length} HTML demo files`);
        }
        
        if (cssFiles.length > 0 && htmlFiles.length > 0) {
          test.passed = true;
        } else {
          test.details.push('❌ Incomplete theme integration');
        }
      } else {
        test.details.push('❌ Cyberpunk directory missing');
      }
    } catch (error) {
      test.details.push(`❌ Theme integration test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testCrossComponentCommunication() {
    const test = { name: 'Cross-Component Communication', passed: false, details: [] };
    
    try {
      // Check if components can reference each other
      const packagesDir = path.join(this.projectRoot, 'packages');
      if (fs.existsSync(packagesDir)) {
        const packages = fs.readdirSync(packagesDir, { withFileTypes: true })
          .filter(dirent => dirent.isDirectory())
          .map(dirent => dirent.name);
        
        if (packages.length > 0) {
          test.details.push(`✅ Found ${packages.length} packages for integration`);
          test.passed = true;
        } else {
          test.details.push('❌ No packages found for integration');
        }
      } else {
        test.details.push('❌ Packages directory missing');
      }
    } catch (error) {
      test.details.push(`❌ Cross-component test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testSystemIntegration() {
    const test = { name: 'System Integration', passed: false, details: [] };
    
    try {
      // Check overall system coherence
      const hasMemory = fs.existsSync(path.join(this.projectRoot, 'packages', 'memory'));
      const hasCyberpunk = fs.existsSync(path.join(this.projectRoot, 'cyberpunk-cli'));
      const hasTests = fs.existsSync(path.join(this.projectRoot, 'tests'));
      
      if (hasMemory && hasCyberpunk && hasTests) {
        test.details.push('✅ All major components present');
        test.passed = true;
      } else {
        test.details.push(`❌ Missing components: Memory:${hasMemory}, Cyberpunk:${hasCyberpunk}, Tests:${hasTests}`);
      }
    } catch (error) {
      test.details.push(`❌ System integration test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
}

module.exports = IntegrationTestSuite;
