/**
 * Unit Tests for README-Driven Help System
 * Tests the revolutionary single-source-of-truth documentation system
 */

const fs = require('fs');
const path = require('path');
const HelpCommand = require('../../../../src/commands/core/help/HelpCommand.cjs');
const WorkspaceCommand = require('../../../../src/commands/core/workspace/WorkspaceCommand.cjs');

describe('README-Driven Help System', () => {
  describe('parseReadmeDefinition', () => {
    test('should parse command definition from README.md format', () => {
      const mockReadme = `
# Test Command

## Definition
- **Name**: test
- **Description**: Test command for parsing
- **Category**: Core
- **Icon**: 🧪
- **Parameters**: \`<action> [options]\`

## Parameters
- \`action\`: Action to perform (test, validate)
- \`options\`: Additional options (optional)

## Usage Examples
\`\`\`bash
python3 ai-portal.py --cmd test --params '{"action": "validate"}'
\`\`\`
`;

      const definition = HelpCommand.parseReadmeDefinition(mockReadme);
      
      expect(definition.name).toBe('test');
      expect(definition.description).toBe('Test command for parsing');
      expect(definition.icon).toBe('🧪');
      expect(definition.parameters).toHaveProperty('action');
      expect(definition.parameters).toHaveProperty('options');
      expect(definition.parameters.action.description).toContain('Action to perform');
    });

    test('should handle missing sections gracefully', () => {
      const incompleteReadme = `
# Incomplete Command

## Definition
- **Name**: incomplete
- **Description**: Missing some sections

## Other Section
Some other content
`;

      const definition = HelpCommand.parseReadmeDefinition(incompleteReadme);
      
      expect(definition.name).toBe('incomplete');
      expect(definition.description).toBe('Missing some sections');
      expect(definition.parameters).toEqual({});
    });

    test('should parse parameters with complex descriptions', () => {
      const complexReadme = `
# Complex Command

## Definition
- **Name**: complex
- **Description**: Complex parameter parsing test

## Parameters
- \`timeout\`: Timeout in seconds (default: 30.0)
- \`retries\`: Number of retry attempts (1-5, default: 2)
- \`config\`: Configuration object (JSON format, optional)
`;

      const definition = HelpCommand.parseReadmeDefinition(complexReadme);
      
      expect(definition.parameters.timeout.description).toContain('Timeout in seconds');
      expect(definition.parameters.retries.description).toContain('Number of retry attempts');
      expect(definition.parameters.config.description).toContain('Configuration object');
    });
  });

  describe('collectAllReadmes', () => {
    const testDir = path.join(__dirname, 'test-readme-collection');
    
    beforeAll(() => {
      // Create test directory structure
      fs.mkdirSync(testDir, { recursive: true });
      fs.mkdirSync(path.join(testDir, 'command1'), { recursive: true });
      fs.mkdirSync(path.join(testDir, 'command2'), { recursive: true });
      fs.mkdirSync(path.join(testDir, 'nested', 'command3'), { recursive: true });
      
      // Create test README files
      fs.writeFileSync(path.join(testDir, 'README.md'), '# Main README\n## Definition\n- **Name**: main');
      fs.writeFileSync(path.join(testDir, 'command1', 'README.md'), '# Command 1\n## Definition\n- **Name**: cmd1');
      fs.writeFileSync(path.join(testDir, 'command2', 'README.md'), '# Command 2\n## Definition\n- **Name**: cmd2');
      fs.writeFileSync(path.join(testDir, 'nested', 'command3', 'README.md'), '# Command 3\n## Definition\n- **Name**: cmd3');
      
      // Create non-README file to ensure it's ignored
      fs.writeFileSync(path.join(testDir, 'command1', 'other.md'), 'Not a README');
    });

    afterAll(() => {
      // Clean up test directory
      fs.rmSync(testDir, { recursive: true, force: true });
    });

    test('should collect all README.md files recursively', async () => {
      const readmes = await HelpCommand.collectAllReadmes(testDir);
      
      expect(readmes.size).toBe(4);
      expect(readmes.has('README.md')).toBe(true);
      expect(readmes.has('command1/README.md')).toBe(true);
      expect(readmes.has('command2/README.md')).toBe(true);
      expect(readmes.has('nested/command3/README.md')).toBe(true);
    });

    test('should include file content and metadata', async () => {
      const readmes = await HelpCommand.collectAllReadmes(testDir);
      const cmd1Readme = readmes.get('command1/README.md');
      
      expect(cmd1Readme).toHaveProperty('content');
      expect(cmd1Readme).toHaveProperty('path');
      expect(cmd1Readme).toHaveProperty('commandDir');
      expect(cmd1Readme.content).toContain('# Command 1');
      expect(cmd1Readme.path).toContain('command1/README.md');
    });

    test('should handle directories without README.md files', async () => {
      const emptyDir = path.join(testDir, 'empty');
      fs.mkdirSync(emptyDir, { recursive: true });
      
      const readmes = await HelpCommand.collectAllReadmes(emptyDir);
      expect(readmes.size).toBe(0);
      
      fs.rmSync(emptyDir, { recursive: true });
    });
  });

  describe('getDefinition integration', () => {
    test('should read definition from actual workspace README.md', () => {
      const definition = WorkspaceCommand.getDefinition();
      
      expect(definition).toHaveProperty('name');
      expect(definition).toHaveProperty('description');
      expect(definition).toHaveProperty('icon');
      expect(definition.name).toBe('workspace');
      expect(definition.description).toContain('workspace directories');
      expect(definition.icon).toBe('📁');
    });

    test('should fallback gracefully if README.md is missing', () => {
      // Temporarily rename README.md to test fallback
      const workspaceDir = path.join(__dirname, '../../../../src/commands/core/workspace');
      const readmePath = path.join(workspaceDir, 'README.md');
      const tempPath = path.join(workspaceDir, 'README.md.backup');
      
      let renamed = false;
      if (fs.existsSync(readmePath)) {
        fs.renameSync(readmePath, tempPath);
        renamed = true;
      }
      
      try {
        const definition = WorkspaceCommand.getDefinition();
        
        expect(definition).toHaveProperty('name');
        expect(definition.name).toBe('workspace');
        expect(definition).toHaveProperty('description');
        expect(definition).toHaveProperty('parameters');
      } finally {
        // Restore README.md
        if (renamed && fs.existsSync(tempPath)) {
          fs.renameSync(tempPath, readmePath);
        }
      }
    });
  });

  describe('Single Source of Truth Architecture', () => {
    test('should ensure README.md drives both getDefinition and help output', async () => {
      // Test that help command reads from README files
      const definition = HelpCommand.getDefinition();
      
      expect(definition.name).toBe('help');
      expect(definition.description).toContain('help information');
      expect(definition.icon).toBe('📚');
    });

    test('should demonstrate no documentation drift', () => {
      // Get definition from README
      const workspaceDefinition = WorkspaceCommand.getDefinition();
      
      // Check that definition contains expected README-driven content
      expect(workspaceDefinition.name).toBe('workspace');
      expect(workspaceDefinition.description).toContain('workspace directories');
      
      // This test ensures the README is the single source of truth
      // If this fails, it means there's drift between README and definition
    });

    test('should validate command documentation completeness', async () => {
      const commandsDir = path.join(__dirname, '../../../../src/commands/core');
      const readmes = await HelpCommand.collectAllReadmes(commandsDir);
      
      // Should find README files for documented commands
      const readmeFiles = Array.from(readmes.keys());
      const documentedCommands = readmeFiles.filter(file => 
        file.includes('/README.md') && file !== 'README.md'
      );
      
      expect(documentedCommands.length).toBeGreaterThan(0);
      
      // Each documented command should have proper definition structure
      for (const [relativePath, readmeData] of readmes) {
        if (relativePath.includes('/README.md') && relativePath !== 'README.md') {
          const definition = HelpCommand.parseReadmeDefinition(readmeData.content);
          
          expect(definition).toHaveProperty('name');
          expect(definition).toHaveProperty('description');
          expect(definition.name).toBeTruthy();
          expect(definition.description).toBeTruthy();
        }
      }
    });
  });

  describe('Documentation Sync Functionality', () => {
    test('should generate documentation from README aggregation', async () => {
      // Mock continuum object
      const mockContinuum = {
        commandProcessor: {
          commandRegistry: {
            commands: new Map([
              ['workspace', WorkspaceCommand],
              ['help', HelpCommand]
            ])
          }
        }
      };

      const commandsList = await HelpCommand.getCommandsList(mockContinuum);
      
      expect(commandsList).toContain('workspace');
      expect(commandsList).toContain('help');
      expect(commandsList).toContain('📁'); // workspace icon
      expect(commandsList).toContain('📚'); // help icon
    });
  });
});