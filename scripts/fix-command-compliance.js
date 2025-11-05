#!/usr/bin/env node
/**
 * Command Module Compliance Auto-Fix Script
 * 
 * Automatically fixes common compliance issues in command modules:
 * - Creates missing package.json files with quality configurations
 * - Creates missing main implementation files (index.ts)
 * - Creates missing test directory structures
 * 
 * Usage: node scripts/fix-command-compliance.js [--dry-run]
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.join(__dirname, '..');

const dryRun = process.argv.includes('--dry-run');

/**
 * Load and process template file
 */
function loadTemplate(templatePath, replacements) {
  const templateContent = fs.readFileSync(templatePath, 'utf8');
  return Object.entries(replacements).reduce((content, [key, value]) => {
    return content.replace(new RegExp(`{{${key}}}`, 'g'), value);
  }, templateContent);
}

/**
 * Create package.json content from template
 */
function createPackageJsonContent(categoryName, description) {
  const templatePath = path.join(__dirname, 'templates', 'command-package.json');
  return loadTemplate(templatePath, {
    CATEGORY: categoryName,
    DESCRIPTION: description
  });
}

/**
 * Create index.ts content from template
 */
function createIndexContent(categoryName) {
  const templatePath = path.join(__dirname, 'templates', 'command-index.ts');
  const categoryTitle = categoryName.charAt(0).toUpperCase() + categoryName.slice(1);
  return loadTemplate(templatePath, {
    CATEGORY: categoryName,
    CATEGORY_TITLE: categoryTitle
  });
}

/**
 * Category descriptions for better package.json descriptions
 */
const categoryDescriptions = {
  'academy': 'Academy AI training and persona management commands',
  'ai': 'AI model interaction and intelligence commands',
  'browser': 'Browser interaction and automation commands',
  'communication': 'Communication and messaging commands',
  'database': 'Database interaction and management commands',
  'development': 'Development workflow and tooling commands',
  'devtools': 'Developer tools and debugging commands',
  'docs': 'Documentation generation and management commands',
  'events': 'Event handling and subscription commands',
  'file': 'File system interaction commands',
  'input': 'User input and interaction commands',
  'kernel': 'System kernel and process management commands',
  'monitoring': 'System monitoring and diagnostics commands',
  'persona': 'Persona management and interaction commands',
  'planning': 'Planning and roadmap management commands',
  'system': 'System-level commands for session management and system control',
  'testing': 'Testing framework and validation commands',
  'ui': 'User interface and widget commands'
};

/**
 * Find command categories that need fixing
 */
function findCommandCategoriesNeedingFixes() {
  const commandsDir = path.join(rootDir, 'src', 'commands');
  const categories = fs.readdirSync(commandsDir, { withFileTypes: true })
    .filter(dirent => dirent.isDirectory())
    .map(dirent => dirent.name)
    .filter(name => name !== 'README.md');

  const fixes = [];

  for (const category of categories) {
    const categoryPath = path.join(commandsDir, category);
    const packageJsonPath = path.join(categoryPath, 'package.json');
    const indexTsPath = path.join(categoryPath, 'index.ts');
    const testDirPath = path.join(categoryPath, 'test');

    const issues = [];
    
    if (!fs.existsSync(packageJsonPath)) {
      issues.push('missing-package-json');
    }
    
    if (!fs.existsSync(indexTsPath)) {
      issues.push('missing-index-ts');
    }
    
    if (!fs.existsSync(testDirPath)) {
      issues.push('missing-test-dir');
    }

    if (issues.length > 0) {
      fixes.push({
        category,
        categoryPath,
        issues,
        packageJsonPath,
        indexTsPath,
        testDirPath
      });
    }
  }

  return fixes;
}

/**
 * Apply fixes to a command category
 */
function applyFixes(fix) {
  const { category, categoryPath, issues, packageJsonPath, indexTsPath, testDirPath } = fix;
  
  console.log(`🔧 Fixing ${category} commands:`);

  // Fix missing package.json
  if (issues.includes('missing-package-json')) {
    const description = categoryDescriptions[category] || `${category} commands`;
    const packageJsonContent = createPackageJsonContent(category, description);
    
    if (!dryRun) {
      fs.writeFileSync(packageJsonPath, packageJsonContent);
    }
    console.log(`   ✅ Created package.json with quality config`);
  }

  // Fix missing index.ts
  if (issues.includes('missing-index-ts')) {
    const indexContent = createIndexContent(category);
    
    if (!dryRun) {
      fs.writeFileSync(indexTsPath, indexContent);
    }
    console.log(`   ✅ Created index.ts entry point`);
  }

  // Fix missing test directory
  if (issues.includes('missing-test-dir')) {
    if (!dryRun) {
      fs.mkdirSync(testDirPath, { recursive: true });
      fs.mkdirSync(path.join(testDirPath, 'unit'), { recursive: true });
      fs.mkdirSync(path.join(testDirPath, 'integration'), { recursive: true });
      
      // Create basic test file from template
      const templatePath = path.join(__dirname, 'templates', 'command-test.ts');
      const categoryTitle = category.charAt(0).toUpperCase() + category.slice(1);
      const testContent = loadTemplate(templatePath, {
        CATEGORY: category,
        CATEGORY_TITLE: categoryTitle
      });
      fs.writeFileSync(path.join(testDirPath, 'unit', `${category}.test.ts`), testContent);
    }
    console.log(`   ✅ Created test directory structure`);
  }

  return issues.length;
}

/**
 * Main execution
 */
function main() {
  console.log('🔍 COMMAND MODULE COMPLIANCE AUTO-FIX');
  console.log('=====================================');
  
  if (dryRun) {
    console.log('🔬 DRY RUN MODE - No files will be modified');
    console.log('');
  }

  const fixes = findCommandCategoriesNeedingFixes();
  
  if (fixes.length === 0) {
    console.log('🎉 All command modules are compliant!');
    return;
  }

  console.log(`🎯 Found ${fixes.length} command categories needing fixes:`);
  console.log('');

  let totalIssuesFixed = 0;

  for (const fix of fixes) {
    const issuesFixed = applyFixes(fix);
    totalIssuesFixed += issuesFixed;
    console.log('');
  }

  console.log('📊 SUMMARY');
  console.log('===========');
  console.log(`📦 Categories processed: ${fixes.length}`);
  console.log(`🔧 Issues fixed: ${totalIssuesFixed}`);
  
  if (!dryRun) {
    console.log('');
    console.log('🎯 NEXT STEPS:');
    console.log('1. Run: npm run test:compliance');
    console.log('2. Review generated files and customize as needed');
    console.log('3. Implement actual command exports in index.ts files');
    console.log('4. Add real tests to replace TODO placeholders');
    console.log('5. Graduate modules by updating quality status');
  }
}

// Run the script
main();