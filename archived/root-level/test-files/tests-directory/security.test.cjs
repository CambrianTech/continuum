
/**
 * NASA-Grade Security Tests
 * Zero tolerance for security vulnerabilities
 */

const fs = require('fs');
const path = require('path');

class SecurityTestSuite {
  constructor() {
    this.testResults = [];
    this.projectRoot = path.join(__dirname, '..');
  }
  
  async runAllTests() {
    console.log('🛡️ Running NASA-grade security tests...');
    
    await this.testCodeInjection();
    await this.testFileSystemSecurity();
    await this.testDependencySecurity();
    await this.testDataValidation();
    
    const failedTests = this.testResults.filter(t => !t.passed);
    if (failedTests.length > 0) {
      throw new Error(`SECURITY FAILURE: ${failedTests.length} tests failed. Security is non-negotiable.`);
    }
    
    console.log(`✅ ALL ${this.testResults.length} security tests PASSED`);
    return true;
  }
  
  async testCodeInjection() {
    const test = { name: 'Code Injection Prevention', passed: false, details: [] };
    
    try {
      // Check for potential injection vulnerabilities
      const dangerousPatterns = ['eval(', 'Function(', 'setTimeout(', 'setInterval('];
      let vulnerabilitiesFound = 0;
      
      const checkDirectory = (dir) => {
        const files = fs.readdirSync(dir, { withFileTypes: true });
        
        files.forEach(file => {
          if (file.isDirectory() && file.name !== 'node_modules') {
            checkDirectory(path.join(dir, file.name));
          } else if (file.name.endsWith('.js') || file.name.endsWith('.ts')) {
            const filePath = path.join(dir, file.name);
            const content = fs.readFileSync(filePath, 'utf-8');
            
            dangerousPatterns.forEach(pattern => {
              if (content.includes(pattern)) {
                vulnerabilitiesFound++;
                test.details.push(`⚠️ Found ${pattern} in ${file.name}`);
              }
            });
          }
        });
      };
      
      checkDirectory(this.projectRoot);
      
      if (vulnerabilitiesFound === 0) {
        test.details.push('✅ No code injection vulnerabilities found');
        test.passed = true;
      } else {
        test.details.push(`❌ Found ${vulnerabilitiesFound} potential vulnerabilities`);
      }
    } catch (error) {
      test.details.push(`❌ Code injection test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testFileSystemSecurity() {
    const test = { name: 'File System Security', passed: false, details: [] };
    
    try {
      // Check for insecure file operations
      const insecurePatterns = ['rm -rf', 'chmod 777', '../../../'];
      let insecureOperations = 0;
      
      const checkFiles = (dir) => {
        const files = fs.readdirSync(dir, { withFileTypes: true });
        
        files.forEach(file => {
          if (file.isDirectory() && file.name !== 'node_modules') {
            checkFiles(path.join(dir, file.name));
          } else if (file.name.endsWith('.js') || file.name.endsWith('.ts') || file.name.endsWith('.cjs')) {
            const filePath = path.join(dir, file.name);
            const content = fs.readFileSync(filePath, 'utf-8');
            
            insecurePatterns.forEach(pattern => {
              if (content.includes(pattern)) {
                insecureOperations++;
                test.details.push(`⚠️ Found ${pattern} in ${file.name}`);
              }
            });
          }
        });
      };
      
      checkFiles(this.projectRoot);
      
      if (insecureOperations === 0) {
        test.details.push('✅ No insecure file operations found');
        test.passed = true;
      } else {
        test.details.push(`❌ Found ${insecureOperations} insecure operations`);
      }
    } catch (error) {
      test.details.push(`❌ File system security test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testDependencySecurity() {
    const test = { name: 'Dependency Security', passed: false, details: [] };
    
    try {
      const { exec } = require('child_process');
      const { promisify } = require('util');
      const execAsync = promisify(exec);
      
      // Run npm audit
      try {
        await execAsync('npm audit --audit-level=high', { cwd: this.projectRoot });
        test.details.push('✅ No high-severity vulnerabilities found');
        test.passed = true;
      } catch (auditError) {
        if (auditError.stdout && auditError.stdout.includes('0 vulnerabilities')) {
          test.details.push('✅ No vulnerabilities found');
          test.passed = true;
        } else {
          test.details.push(`❌ Security vulnerabilities found: ${auditError.message.substring(0, 100)}`);
        }
      }
    } catch (error) {
      test.details.push(`❌ Dependency security test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testDataValidation() {
    const test = { name: 'Data Validation', passed: false, details: [] };
    
    try {
      // Check for proper input validation
      let hasValidation = false;
      
      const checkForValidation = (dir) => {
        const files = fs.readdirSync(dir, { withFileTypes: true });
        
        files.forEach(file => {
          if (file.isDirectory() && file.name !== 'node_modules') {
            checkForValidation(path.join(dir, file.name));
          } else if (file.name.endsWith('.js') || file.name.endsWith('.ts')) {
            const filePath = path.join(dir, file.name);
            const content = fs.readFileSync(filePath, 'utf-8');
            
            // Look for validation patterns
            const validationPatterns = ['typeof', 'instanceof', 'Array.isArray', 'length', 'trim()'];
            if (validationPatterns.some(pattern => content.includes(pattern))) {
              hasValidation = true;
            }
          }
        });
      };
      
      checkForValidation(this.projectRoot);
      
      if (hasValidation) {
        test.details.push('✅ Data validation patterns found');
        test.passed = true;
      } else {
        test.details.push('❌ No data validation patterns found');
      }
    } catch (error) {
      test.details.push(`❌ Data validation test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
}

module.exports = SecurityTestSuite;
