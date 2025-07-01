/**
 * Module Migration System - handles all migration logic
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { MigrationResult } from './BaseModule.js';

export class ModuleMigration {
  protected modulePath: string;
  protected packageJson?: any;

  constructor(modulePath: string, packageJson?: any) {
    this.modulePath = modulePath;
    this.packageJson = packageJson;
  }

  /**
   * Smart migration - always migrates to current version
   * Validates after migration to confirm success
   */
  async migrate(): Promise<MigrationResult> {
    const fromVersion = await this.detectCurrentState();
    
    const result: MigrationResult = {
      migrated: false,
      changes: [],
      errors: [],
      versionFrom: fromVersion,
      versionTo: 'current'
    };

    try {
      // Smart migration based on current state
      if (fromVersion === 'unknown') {
        console.log(`🔧 Bootstrapping ${this.getModuleId()} from scratch`);
        await this.bootstrapFromNothing(result);
      } else if (fromVersion === 'legacy') {
        console.log(`🔄 Modernizing ${this.getModuleId()} from legacy`);
        await this.modernizeFromLegacy(result);
      } else if (fromVersion === 'partial') {
        console.log(`✨ Completing ${this.getModuleId()} structure`);
        await this.incrementalUpdate(result);
      } else {
        console.log(`✅ ${this.getModuleId()} already current`);
        // Even if current, check for stock READMEs that should be upgraded
        await this.upgradeStockFiles(result);
        if (!result.migrated) {
          return result; // No migration needed, skip validation
        }
      }

      // VALIDATE AFTER MIGRATION - use BaseModule.validate()
      if (result.migrated) {
        try {
          const { BaseModule } = await import('./BaseModule.js');
          const module = new BaseModule(this.modulePath);
          const validation = await module.validate();
          
          if (validation.isValid) {
            console.log(`   ✅ Migration validated - module now compliant`);
          } else {
            console.log(`   ❌ Migration validation failed - ${validation.errors.length} issues remain`);
            validation.errors.slice(0, 3).forEach(error => console.log(`      - ${error}`));
            result.errors.push(...validation.errors.map(e => `Validation: ${e}`));
            
            // Migration claimed success but validation failed
            result.migrated = false;
          }
        } catch (error) {
          console.log(`   ⚠️  Could not validate after migration: ${error instanceof Error ? error.message : String(error)}`);
          // Don't fail migration if validation itself fails
        }
      }

    } catch (error) {
      result.errors.push(`Migration failed: ${error instanceof Error ? error.message : String(error)}`);
    }

    return result;
  }


  /**
   * Detect current module state intelligently
   */
  protected async detectCurrentState(): Promise<string> {
    // No package.json = unknown/bootstrap needed
    if (!this.packageJson) return 'unknown';
    
    // Has package.json but missing continuum config = legacy
    if (!this.packageJson.continuum) return 'legacy';
    
    // Has continuum config but missing test dirs = partial
    if (!(await this.checkDirectoryExists('test'))) return 'partial';
    
    // Otherwise it's at some version
    return this.packageJson.version || '1.0.0';
  }

  /**
   * Bootstrap module from nothing - create everything
   */
  protected async bootstrapFromNothing(result: MigrationResult): Promise<void> {
    console.log(`   🏗️  Creating full module structure`);
    
    // Create all directories
    await this.fixDirectories(result);
    
    // Generate all missing files
    await this.fixFiles(result);
    
    // Fix package.json completely
    await this.fixPackageJson(result);
    
    // Generate continuum config if missing
    await this.generateContinuumConfig(result);
  }

  /**
   * Modernize from legacy module
   */
  protected async modernizeFromLegacy(result: MigrationResult): Promise<void> {
    console.log(`   🔄 Modernizing legacy module`);
    
    // Add missing modern structure
    await this.fixDirectories(result);
    await this.fixFiles(result);
    
    // Add continuum config to existing package.json
    await this.generateContinuumConfig(result);
  }

  /**
   * Incremental update between versions
   */
  protected async incrementalUpdate(result: MigrationResult): Promise<void> {
    console.log(`   ✨ Applying incremental updates`);
    
    // Only fix what's actually missing - would need validation logic here
    await this.fixDirectories(result);
    await this.fixFiles(result);
  }

  /**
   * Upgrade stock files even in current modules
   */
  protected async upgradeStockFiles(result: MigrationResult): Promise<void> {
    console.log(`   🔍 Checking for stock files to upgrade`);
    
    // Check for stock README that should be upgraded
    if (await this.checkFileExists('README.md')) {
      const shouldRegenerate = await this.isStockReadme('README.md');
      
      if (shouldRegenerate.shouldReplace) {
        console.log(`   📝 README.md is ${shouldRegenerate.reason} - upgrading to modern template`);
        await this.generateReadme();
        result.changes.push(`Upgraded ${shouldRegenerate.reason} README.md to modern template`);
        result.migrated = true;
      }
    }
  }

  protected async fixDirectories(result: MigrationResult): Promise<void> {
    const dirs = ['test', 'test/unit', 'test/integration'];
    
    for (const dir of dirs) {
      if (!await this.checkDirectoryExists(dir)) {
        await fs.mkdir(path.join(this.modulePath, dir), { recursive: true });
        result.changes.push(`Created ${dir} directory`);
        result.migrated = true;
      }
    }
  }

  protected async fixFiles(result: MigrationResult): Promise<void> {
    // Fix README.md - but be careful not to overwrite customized content
    if (!await this.checkFileExists('README.md')) {
      await this.generateReadme();
      result.changes.push('Generated README.md');
      result.migrated = true;
    } else {
      // Check if existing README is auto-generated/stock
      const shouldRegenerate = await this.isStockReadme('README.md');
      
      if (shouldRegenerate.shouldReplace) {
        console.log(`   📝 README.md is ${shouldRegenerate.reason} - regenerating...`);
        await this.generateReadme();
        result.changes.push(`Regenerated ${shouldRegenerate.reason} README.md`);
        result.migrated = true;
      } else {
        const existingSize = await this.getFileSize('README.md');
        console.log(`   ✋ README.md is customized (${existingSize} bytes) - preserving`);
      }
    }
  }

  protected async fixPackageJson(result: MigrationResult): Promise<void> {
    if (!this.packageJson) return;

    let updated = false;

    // Add test scripts if missing - using tsx for TypeScript modules
    if (!this.packageJson.scripts?.test) {
      if (!this.packageJson.scripts) this.packageJson.scripts = {};
      this.packageJson.scripts.test = 'npx tsx test/';
      this.packageJson.scripts['test:unit'] = 'npx tsx test/unit/';
      this.packageJson.scripts['test:integration'] = 'npx tsx test/integration/';
      updated = true;
    }

    if (updated) {
      await this.savePackageJson();
      result.changes.push('Added test scripts to package.json');
      result.migrated = true;
    }
  }

  protected async generateContinuumConfig(result: MigrationResult): Promise<void> {
    if (!this.packageJson) {
      // Generate entire package.json from template
      await this.generateSmartPackageJson(result);
      return;
    }
    
    // Only add continuum config if missing
    if (!this.packageJson.continuum) {
      const smartConfig = this.generateSmartContinuumConfig();
      this.packageJson.continuum = smartConfig;
      
      await this.savePackageJson();
      result.changes.push('Generated smart continuum configuration');
      result.migrated = true;
    }
    
    // Ensure TypeScript setup
    if (!this.packageJson.type || this.packageJson.type !== 'module') {
      this.packageJson.type = 'module';
      await this.savePackageJson();
      result.changes.push('Set module type to ES modules for TypeScript');
      result.migrated = true;
    }
  }

  /**
   * Generate complete package.json from template
   */
  protected async generateSmartPackageJson(result: MigrationResult): Promise<void> {
    const moduleId = this.getModuleId();
    const smartConfig = this.generateSmartContinuumConfig();
    
    // Create new package.json with smart defaults
    this.packageJson = {
      name: `${moduleId}-${smartConfig.category.toLowerCase()}`,
      version: '1.0.0',
      description: `${moduleId} ${smartConfig.category.toLowerCase()} for Continuum`,
      main: `${moduleId.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join('')}${smartConfig.category}.ts`,
      type: 'module',
      continuum: smartConfig,
      keywords: [moduleId, smartConfig.category.toLowerCase(), 'continuum'],
      author: 'Continuum AI Platform',
      license: 'MIT',
      scripts: {
        test: 'npx tsx test/',
        'test:unit': 'npx tsx test/unit/',
        'test:integration': 'npx tsx test/integration/'
      },
      engines: {
        node: '>=18.0.0'
      }
    };
    
    await this.savePackageJson();
    result.changes.push('Generated complete package.json from template');
    result.migrated = true;
  }

  /**
   * Generate smart continuum configuration based on module location
   */
  protected generateSmartContinuumConfig() {
    const moduleId = this.getModuleId();
    const modulePath = this.modulePath.replace('./src/', '');
    const parts = modulePath.split('/');
    
    // Smart category detection
    const category = parts[0] === 'commands' ? 'Command' : 
                    parts[0] === 'daemons' ? 'Daemon' :
                    parts[0] === 'ui' ? 'UI' : 'Core';
    
    // Smart capability detection based on name patterns
    const capabilities = [];
    if (moduleId.includes('read') || moduleId.includes('file')) capabilities.push('file-reading');
    if (moduleId.includes('write') || moduleId.includes('save')) capabilities.push('file-writing');
    if (moduleId.includes('chat') || moduleId.includes('message')) capabilities.push('messaging');
    if (moduleId.includes('ui') || moduleId.includes('widget')) capabilities.push('ui-rendering');
    if (moduleId.includes('test') || moduleId.includes('validate')) capabilities.push('testing');
    if (capabilities.length === 0) capabilities.push('auto-generated');
    
    // Smart interface detection
    const interfaces = [];
    if (category === 'Command') interfaces.push('CommandInterface');
    if (category === 'Daemon') interfaces.push('DaemonInterface');
    if (category === 'UI') interfaces.push('WidgetInterface');
    
    // Smart permission detection
    const permissions = [];
    if (capabilities.includes('file-reading')) permissions.push('read');
    if (capabilities.includes('file-writing')) permissions.push('write');
    if (capabilities.includes('messaging')) permissions.push('network');
    
    return {
      [category.toLowerCase()]: moduleId.replace(/-/g, '_'),
      category,
      capabilities,
      dependencies: [],
      interfaces,
      permissions
    };
  }

  protected async generateReadme(): Promise<void> {
    const moduleId = this.getModuleId();
    const description = this.packageJson?.description || `${moduleId} module for Continuum`;
    
    // Read template
    let template;
    try {
      template = await fs.readFile(path.join(__dirname, 'templates/README-template.md'), 'utf8');
    } catch {
      // Fallback template if template file not found - make it comprehensive
      template = `# {{MODULE_TITLE}}

{{DESCRIPTION}}

## 🚀 Usage

### Command Interface
\`\`\`bash
# Basic usage
continuum {{MODULE_COMMAND}}

# With options (customize based on your module)
continuum {{MODULE_COMMAND}} --help
continuum {{MODULE_COMMAND}} --verbose
\`\`\`

### Programmatic Usage
\`\`\`typescript
import { {{MODULE_CLASS}} } from './{{MODULE_FILE}}.js';

// Execute the command
const result = await {{MODULE_CLASS}}.execute({
  // Add your parameters here
});

console.log(result);
\`\`\`

## ⚙️ Configuration

\`\`\`json
{{CONTINUUM_CONFIG}}
\`\`\`

## 🧪 Testing

\`\`\`bash
# Run all tests
npm test

# Run specific test types
npm run test:unit
npm run test:integration

# Validate module compliance
npm run validate
\`\`\`

## 🏗️ Development

This module follows the Continuum modular architecture:

- **Self-validating**: Module validates its own compliance
- **Middle-out**: Tests from core outward 
- **Object-oriented**: Inherits from base classes
- **Migration-ready**: Can upgrade structure automatically

### Module Structure
\`\`\`
{{MODULE_COMMAND}}/
├── {{MODULE_FILE}}.ts     # Main implementation
├── test/
│   ├── unit/             # Unit tests
│   └── integration/      # Integration tests
├── package.json          # Module configuration
└── README.md            # This file
\`\`\`

## 📋 Implementation Notes

**TODO**: Customize this section with:
- Specific usage examples
- Configuration options
- API documentation
- Performance considerations
- Known limitations

## 🔧 Bootstrap Information

This file was auto-generated during module migration. The module now has:

- ✅ Complete package.json with continuum configuration
- ✅ Test directories (unit/integration)
- ✅ TypeScript ES module setup
- ✅ Compliance validation

**Next Steps**: Implement your module logic and update this documentation!`;
    }
    
    // Smart replacements
    const moduleCommand = this.packageJson?.continuum?.command || moduleId;
    const moduleClass = moduleId.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join('') + 'Command';
    const moduleFile = moduleId.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join('') + 'Command';
    
    // Get the continuum config for the README (re-read package.json to get latest)
    let currentPackageJson = this.packageJson;
    try {
      const packagePath = path.join(this.modulePath, 'package.json');
      const packageContent = await fs.readFile(packagePath, 'utf8');
      currentPackageJson = JSON.parse(packageContent);
    } catch {
      // Use existing if can't re-read
    }

    const content = template
      .replace(/{{MODULE_TITLE}}/g, moduleId.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '))
      .replace(/{{DESCRIPTION}}/g, description)
      .replace(/{{MODULE_COMMAND}}/g, moduleCommand)
      .replace(/{{MODULE_CLASS}}/g, moduleClass)
      .replace(/{{MODULE_FILE}}/g, moduleFile)
      .replace(/{{CONTINUUM_CONFIG}}/g, currentPackageJson?.continuum ? JSON.stringify(currentPackageJson.continuum, null, 2) : '// Add continuum configuration');

    await fs.writeFile(path.join(this.modulePath, 'README.md'), content);
  }

  // Helper methods
  protected async checkFileExists(fileName: string): Promise<boolean> {
    try {
      await fs.access(path.join(this.modulePath, fileName));
      return true;
    } catch {
      return false;
    }
  }

  protected async getFileSize(fileName: string): Promise<number> {
    try {
      const stat = await fs.stat(path.join(this.modulePath, fileName));
      return stat.size;
    } catch {
      return 0;
    }
  }

  /**
   * Smart detection of stock/auto-generated READMEs that should be replaced
   */
  protected async isStockReadme(fileName: string): Promise<{shouldReplace: boolean, reason: string}> {
    try {
      const content = await fs.readFile(path.join(this.modulePath, fileName), 'utf8');
      
      // Stock phrases that indicate auto-generated content
      const stockPhrases = [
        'Bootstrap-generated file - customize as needed',
        'Auto-generated file - customize as needed', 
        '## Bootstrap-generated file',
        '## Auto-generated file',
        'customize as needed',
        '# Add usage examples here',
        '// Add continuum configuration',
        '(remove this message at the top to customize)',
        'remove this message',
        'customize this file',
        'TODO: Add description',
        'TODO: Implement'
      ];
      
      // Check for stock phrases
      for (const phrase of stockPhrases) {
        if (content.includes(phrase)) {
          return { shouldReplace: true, reason: 'stock template' };
        }
      }
      
      // Check for very minimal content (likely auto-generated)
      const lines = content.split('\n').filter(line => line.trim());
      if (lines.length < 10) {
        return { shouldReplace: true, reason: 'minimal content' };
      }
      
      // Check for repetitive basic structure
      const hasBasicStructure = content.includes('## Usage') && 
                               content.includes('## Testing') && 
                               content.includes('npm test') &&
                               lines.length < 20;
      
      if (hasBasicStructure) {
        return { shouldReplace: true, reason: 'basic template' };
      }
      
      return { shouldReplace: false, reason: 'customized' };
      
    } catch {
      return { shouldReplace: false, reason: 'read error' };
    }
  }

  protected async checkDirectoryExists(dirName: string): Promise<boolean> {
    try {
      const stat = await fs.stat(path.join(this.modulePath, dirName));
      return stat.isDirectory();
    } catch {
      return false;
    }
  }

  protected getModuleId(): string {
    return this.packageJson?.continuum?.module || 
           this.packageJson?.name ||
           path.basename(this.modulePath);
  }

  protected async savePackageJson(): Promise<void> {
    if (this.packageJson) {
      const packagePath = path.join(this.modulePath, 'package.json');
      await fs.writeFile(packagePath, JSON.stringify(this.packageJson, null, 2));
    }
  }
}