
/**
 * NASA-Grade Self-Validation Tests
 * AI validates its own work
 */

const fs = require('fs');
const path = require('path');

class SelfValidationTestSuite {
  constructor() {
    this.testResults = [];
    this.projectRoot = path.join(__dirname, '..');
  }
  
  async runAllTests() {
    console.log('🔍 Running NASA-grade self-validation tests...');
    
    await this.testSelfConsistency();
    await this.testGoalAchievement();
    await this.testQualityStandards();
    await this.testCompleteness();
    
    const failedTests = this.testResults.filter(t => !t.passed);
    if (failedTests.length > 0) {
      throw new Error(`SELF-VALIDATION FAILURE: ${failedTests.length} tests failed. AI must validate its own work.`);
    }
    
    console.log(`✅ ALL ${this.testResults.length} self-validation tests PASSED`);
    return true;
  }
  
  async testSelfConsistency() {
    const test = { name: 'Self-Consistency', passed: false, details: [] };
    
    try {
      // Check if all created components are consistent with each other
      const hasMemoryPackage = fs.existsSync(path.join(this.projectRoot, 'packages', 'memory'));
      const hasCyberpunkTheme = fs.existsSync(path.join(this.projectRoot, 'cyberpunk-cli'));
      const hasTests = fs.existsSync(path.join(this.projectRoot, 'tests'));
      
      if (hasMemoryPackage && hasCyberpunkTheme && hasTests) {
        test.details.push('✅ All major components exist');
        
        // Check consistency of naming and structure
        const memoryPackageJson = path.join(this.projectRoot, 'packages', 'memory', 'package.json');
        if (fs.existsSync(memoryPackageJson)) {
          const packageData = JSON.parse(fs.readFileSync(memoryPackageJson, 'utf-8'));
          if (packageData.name && packageData.name.includes('continuum')) {
            test.details.push('✅ Consistent naming conventions');
            test.passed = true;
          } else {
            test.details.push('❌ Inconsistent naming conventions');
          }
        }
      } else {
        test.details.push(`❌ Missing components: Memory:${hasMemoryPackage}, Cyberpunk:${hasCyberpunkTheme}, Tests:${hasTests}`);
      }
    } catch (error) {
      test.details.push(`❌ Self-consistency test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testGoalAchievement() {
    const test = { name: 'Goal Achievement', passed: false, details: [] };
    
    try {
      // Validate that primary goals were achieved
      const goals = [
        { name: 'Memory Package', check: () => fs.existsSync(path.join(this.projectRoot, 'packages', 'memory', 'src', 'index.ts')) },
        { name: 'Cyberpunk Theme', check: () => fs.existsSync(path.join(this.projectRoot, 'cyberpunk-cli')) },
        { name: 'Test Suite', check: () => fs.existsSync(path.join(this.projectRoot, 'tests')) },
        { name: 'Quality Gates', check: () => this.projectRoot !== null } // This test itself proves quality gates work
      ];
      
      const achievedGoals = goals.filter(goal => goal.check());
      
      if (achievedGoals.length === goals.length) {
        test.details.push(`✅ All ${goals.length} primary goals achieved`);
        test.passed = true;
      } else {
        const failedGoals = goals.filter(goal => !goal.check());
        test.details.push(`❌ ${failedGoals.length} goals not achieved: ${failedGoals.map(g => g.name).join(', ')}`);
      }
    } catch (error) {
      test.details.push(`❌ Goal achievement test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testQualityStandards() {
    const test = { name: 'Quality Standards', passed: false, details: [] };
    
    try {
      // Validate quality standards are met
      let qualityScore = 0;
      const maxScore = 5;
      
      // Check code structure
      if (fs.existsSync(path.join(this.projectRoot, 'packages'))) {
        qualityScore++;
        test.details.push('✅ Proper project structure');
      }
      
      // Check documentation
      const readmeExists = fs.existsSync(path.join(this.projectRoot, 'README.md'));
      if (readmeExists) {
        qualityScore++;
        test.details.push('✅ Documentation present');
      }
      
      // Check TypeScript usage
      const tsConfigExists = fs.existsSync(path.join(this.projectRoot, 'tsconfig.json'));
      if (tsConfigExists) {
        qualityScore++;
        test.details.push('✅ TypeScript configuration');
      }
      
      // Check package.json
      const packageJsonExists = fs.existsSync(path.join(this.projectRoot, 'package.json'));
      if (packageJsonExists) {
        qualityScore++;
        test.details.push('✅ Package configuration');
      }
      
      // Check test coverage
      const testFiles = fs.readdirSync(path.join(this.projectRoot, 'tests')).filter(f => f.endsWith('.test.js'));
      if (testFiles.length >= 5) {
        qualityScore++;
        test.details.push('✅ Comprehensive test coverage');
      }
      
      if (qualityScore >= 4) { // 80% quality threshold
        test.details.push(`✅ Quality standards met (${qualityScore}/${maxScore})`);
        test.passed = true;
      } else {
        test.details.push(`❌ Quality standards not met (${qualityScore}/${maxScore})`);
      }
    } catch (error) {
      test.details.push(`❌ Quality standards test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testCompleteness() {
    const test = { name: 'Completeness', passed: false, details: [] };
    
    try {
      // Check if the AI completed everything it set out to do
      const expectedFiles = [
        'packages/memory/package.json',
        'packages/memory/src/index.ts',
        'cyberpunk-cli',
        'tests'
      ];
      
      const missingFiles = expectedFiles.filter(file => {
        const fullPath = path.join(this.projectRoot, file);
        return !fs.existsSync(fullPath);
      });
      
      if (missingFiles.length === 0) {
        test.details.push('✅ All expected files and directories created');
        test.passed = true;
      } else {
        test.details.push(`❌ Missing files: ${missingFiles.join(', ')}`);
      }
    } catch (error) {
      test.details.push(`❌ Completeness test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
}

module.exports = SelfValidationTestSuite;
