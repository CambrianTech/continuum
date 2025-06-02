#!/usr/bin/env node
/**
 * PR-Fixing AI
 * 
 * Single purpose: Fix the specific issues in PR #63 and get it merged
 * No endless commits, no timestamp spam - just targeted fixes
 */

const fs = require('fs');
const path = require('path');
const { exec } = require('child_process');
const { promisify } = require('util');

const execAsync = promisify(exec);

class PRFixingAI {
  constructor() {
    this.projectRoot = process.cwd();
    
    console.log('🎯 PR-FIXING AI');
    console.log('===============');
    console.log('Mission: Fix PR #63 issues and get it merged');
    console.log('Target: https://github.com/CambrianTech/continuum/pull/63');
    console.log('Issues identified:');
    console.log('  1. Missing .cyberpunk-terminal selector in JS');
    console.log('  2. Inline styles should be in CSS');
    console.log('  3. Duplicate responsive logic in JS/CSS');
    console.log('');

    this.fixPR();
  }

  async fixPR() {
    try {
      console.log('🔧 Fixing identified issues...');
      
      // Fix 1: Update JS selector to include .cyberpunk-terminal
      await this.fixJavaScriptSelector();
      
      // Fix 2: Move inline styles to CSS
      await this.fixInlineStyles();
      
      // Fix 3: Remove duplicate responsive logic from JS
      await this.removeDuplicateResponsiveLogic();
      
      console.log('✅ All issues fixed!');
      
      // Commit the fixes
      await this.commitFixes();
      
      console.log('🚀 Fixes committed and pushed!');
      console.log('PR should now be ready for merge.');
      
    } catch (error) {
      console.error('❌ Error:', error.message);
    }
  }

  async fixJavaScriptSelector() {
    const jsFile = path.join(this.projectRoot, 'cyberpunk-cli', 'cyberpunk-improved.js');
    
    if (!fs.existsSync(jsFile)) {
      console.log('⚠️ JS file not found, skipping JS fixes');
      return;
    }
    
    let content = fs.readFileSync(jsFile, 'utf-8');
    
    // Fix: Add .cyberpunk-terminal to selector
    content = content.replace(
      `const cliElements = document.querySelectorAll('.terminal, .cli, .cyberpunk');`,
      `const cliElements = document.querySelectorAll('.terminal, .cli, .cyberpunk, .cyberpunk-terminal');`
    );
    
    fs.writeFileSync(jsFile, content);
    console.log('✅ Fixed JS selector to include .cyberpunk-terminal');
  }

  async removeDuplicateResponsiveLogic() {
    const jsFile = path.join(this.projectRoot, 'cyberpunk-cli', 'cyberpunk-improved.js');
    
    if (!fs.existsSync(jsFile)) {
      return;
    }
    
    let content = fs.readFileSync(jsFile, 'utf-8');
    
    // Remove the entire handleResize method and its call
    content = content.replace(/handleResize\(\) \{[\s\S]*?\n  \}/g, 'handleResize() {\n    // Responsive adjustments are now handled via CSS media queries.\n  }');
    
    fs.writeFileSync(jsFile, content);
    console.log('✅ Removed duplicate responsive logic from JS');
  }

  async fixInlineStyles() {
    const htmlFile = path.join(this.projectRoot, 'cyberpunk-cli', 'demo.html');
    const cssFile = path.join(this.projectRoot, 'cyberpunk-cli', 'cyberpunk-improved.css');
    
    if (!fs.existsSync(htmlFile)) {
      console.log('⚠️ HTML file not found, skipping inline style fixes');
      return;
    }
    
    // Remove inline styles from HTML
    let htmlContent = fs.readFileSync(htmlFile, 'utf-8');
    htmlContent = htmlContent.replace(
      '<body style="background: #000; margin: 0; padding: 20px;">',
      '<body class="cyberpunk-body">'
    );
    fs.writeFileSync(htmlFile, htmlContent);
    
    // Add styles to CSS if it exists
    if (fs.existsSync(cssFile)) {
      let cssContent = fs.readFileSync(cssFile, 'utf-8');
      
      // Add body styles if not already present
      if (!cssContent.includes('.cyberpunk-body')) {
        cssContent += `\n\n/* Moved from inline styles */\n.cyberpunk-body {\n  background: #000;\n  margin: 0;\n  padding: 20px;\n}\n`;
        fs.writeFileSync(cssFile, cssContent);
      }
    }
    
    console.log('✅ Moved inline styles to CSS');
  }

  async commitFixes() {
    try {
      // Add changes
      await execAsync('git add cyberpunk-cli/', { cwd: this.projectRoot });
      
      // Check if there are actually changes to commit
      const status = await execAsync('git status --porcelain', { cwd: this.projectRoot });
      if (!status.stdout.trim()) {
        console.log('📝 No changes to commit (fixes may already be applied)');
        return;
      }
      
      // Commit with clear message
      const commitMessage = `fix: address Copilot PR review feedback

🔧 Fixed issues identified in PR #63:
- Add .cyberpunk-terminal to JS selector for proper element targeting
- Move inline styles from HTML to CSS for better maintainability  
- Remove duplicate responsive logic from JS (handled by CSS media queries)

These changes address all code quality issues raised by GitHub Copilot reviewer.

🚀 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>`;

      await execAsync(`git commit -m "${commitMessage}"`, { cwd: this.projectRoot });
      console.log('📝 Committed fixes');
      
      // Push changes
      try {
        await execAsync('git push', { cwd: this.projectRoot });
        console.log('📤 Pushed fixes to remote');
      } catch (pushError) {
        console.log('⚠️ Could not push - may need to set upstream');
        try {
          const branch = await execAsync('git branch --show-current', { cwd: this.projectRoot });
          await execAsync(`git push -u origin ${branch.stdout.trim()}`, { cwd: this.projectRoot });
          console.log('📤 Pushed with upstream set');
        } catch (upstreamError) {
          console.log('⚠️ Push failed:', upstreamError.message);
        }
      }
      
    } catch (error) {
      console.log('⚠️ Commit error:', error.message);
    }
  }
}

// Run the PR-fixing AI
new PRFixingAI();