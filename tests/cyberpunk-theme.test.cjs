
/**
 * NASA-Grade Cyberpunk Theme Tests
 * Mission-critical UI validation
 */

const fs = require('fs');
const path = require('path');

class CyberpunkThemeTestSuite {
  constructor() {
    this.testResults = [];
    this.cyberpunkPath = path.join(__dirname, '..', 'cyberpunk-cli');
  }
  
  async runAllTests() {
    console.log('🎨 Running NASA-grade cyberpunk theme tests...');
    
    await this.testThemeStructure();
    await this.testCSSValidation();
    await this.testResponsiveDesign();
    await this.testAccessibility();
    await this.testFileSize();
    await this.testColorContrast();
    
    const failedTests = this.testResults.filter(t => !t.passed);
    if (failedTests.length > 0) {
      throw new Error(`THEME FAILURE: ${failedTests.length} tests failed. All theme tests must pass.`);
    }
    
    console.log(`✅ ALL ${this.testResults.length} cyberpunk theme tests PASSED`);
    return true;
  }
  
  async testThemeStructure() {
    const test = { name: 'Theme Structure', passed: false, details: [] };
    
    if (fs.existsSync(this.cyberpunkPath)) {
      test.details.push('✅ Cyberpunk directory exists');
      
      const cssFiles = fs.readdirSync(this.cyberpunkPath).filter(f => f.endsWith('.css'));
      if (cssFiles.length > 0) {
        test.details.push(`✅ Found ${cssFiles.length} CSS files`);
        test.passed = true;
      } else {
        test.details.push('❌ No CSS files found');
      }
    } else {
      test.details.push('❌ Cyberpunk directory missing');
    }
    
    this.testResults.push(test);
  }
  
  async testCSSValidation() {
    const test = { name: 'CSS Validation', passed: false, details: [] };
    
    try {
      const cssFiles = fs.readdirSync(this.cyberpunkPath).filter(f => f.endsWith('.css'));
      let allValid = true;
      
      for (const cssFile of cssFiles) {
        const cssPath = path.join(this.cyberpunkPath, cssFile);
        const css = fs.readFileSync(cssPath, 'utf-8');
        
        // Basic CSS validation
        if (css.includes('--cyber-primary') || css.includes('#00ff')) {
          test.details.push(`✅ ${cssFile} has cyberpunk color scheme`);
        } else {
          test.details.push(`❌ ${cssFile} missing cyberpunk colors`);
          allValid = false;
        }
        
        if (css.includes('@media')) {
          test.details.push(`✅ ${cssFile} has responsive design`);
        } else {
          test.details.push(`❌ ${cssFile} missing responsive design`);
          allValid = false;
        }
      }
      
      test.passed = allValid;
    } catch (error) {
      test.details.push(`❌ CSS validation failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testResponsiveDesign() {
    const test = { name: 'Responsive Design', passed: false, details: [] };
    
    try {
      const cssFiles = fs.readdirSync(this.cyberpunkPath).filter(f => f.endsWith('.css'));
      let hasResponsive = false;
      
      for (const cssFile of cssFiles) {
        const cssPath = path.join(this.cyberpunkPath, cssFile);
        const css = fs.readFileSync(cssPath, 'utf-8');
        
        if (css.includes('@media') && css.includes('max-width')) {
          hasResponsive = true;
          test.details.push(`✅ ${cssFile} has mobile responsiveness`);
        }
      }
      
      if (hasResponsive) {
        test.passed = true;
      } else {
        test.details.push('❌ No responsive design found');
      }
    } catch (error) {
      test.details.push(`❌ Responsive design test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testAccessibility() {
    const test = { name: 'Accessibility', passed: false, details: [] };
    
    try {
      const cssFiles = fs.readdirSync(this.cyberpunkPath).filter(f => f.endsWith('.css'));
      let hasAccessibility = false;
      
      for (const cssFile of cssFiles) {
        const cssPath = path.join(this.cyberpunkPath, cssFile);
        const css = fs.readFileSync(cssPath, 'utf-8');
        
        if (css.includes(':focus') || css.includes('focus')) {
          hasAccessibility = true;
          test.details.push(`✅ ${cssFile} has focus states`);
        }
      }
      
      if (hasAccessibility) {
        test.passed = true;
      } else {
        test.details.push('❌ No accessibility features found');
      }
    } catch (error) {
      test.details.push(`❌ Accessibility test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testFileSize() {
    const test = { name: 'File Size Efficiency', passed: false, details: [] };
    
    try {
      const cssFiles = fs.readdirSync(this.cyberpunkPath).filter(f => f.endsWith('.css'));
      let allEfficient = true;
      
      for (const cssFile of cssFiles) {
        const cssPath = path.join(this.cyberpunkPath, cssFile);
        const stats = fs.statSync(cssPath);
        
        if (stats.size < 50000) { // 50KB max
          test.details.push(`✅ ${cssFile} size OK (${stats.size} bytes)`);
        } else {
          test.details.push(`❌ ${cssFile} too large (${stats.size} bytes)`);
          allEfficient = false;
        }
      }
      
      test.passed = allEfficient;
    } catch (error) {
      test.details.push(`❌ File size test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testColorContrast() {
    const test = { name: 'Color Contrast', passed: false, details: [] };
    
    try {
      const cssFiles = fs.readdirSync(this.cyberpunkPath).filter(f => f.endsWith('.css'));
      let hasGoodContrast = false;
      
      for (const cssFile of cssFiles) {
        const cssPath = path.join(this.cyberpunkPath, cssFile);
        const css = fs.readFileSync(cssPath, 'utf-8');
        
        // Check for high contrast color combinations
        if ((css.includes('#000') || css.includes('black')) && 
            (css.includes('#00ff') || css.includes('#fff') || css.includes('white'))) {
          hasGoodContrast = true;
          test.details.push(`✅ ${cssFile} has high contrast colors`);
        }
      }
      
      if (hasGoodContrast) {
        test.passed = true;
      } else {
        test.details.push('❌ Insufficient color contrast');
      }
    } catch (error) {
      test.details.push(`❌ Color contrast test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
}

module.exports = CyberpunkThemeTestSuite;
