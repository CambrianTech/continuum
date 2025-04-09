import { loadConfig, validateConfig, mergeConfigs, AIConfig } from '../src';
import * as fs from 'fs/promises';

// Mock fs
jest.mock('fs/promises', () => ({
  readFile: jest.fn(),
  writeFile: jest.fn(),
  access: jest.fn(),
}));

describe('Core functionality', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe('loadConfig', () => {
    it('should load YAML configuration', async () => {
      const mockYaml = `
ai_protocol_version: '0.1'
identity:
  name: TestAssistant
  role: Tester
`;
      (fs.readFile as jest.Mock).mockResolvedValue(mockYaml);
      
      const config = await loadConfig('config.yml');
      
      expect(fs.readFile).toHaveBeenCalledWith('config.yml', 'utf-8');
      expect(config).toEqual({
        ai_protocol_version: '0.1',
        identity: {
          name: 'TestAssistant',
          role: 'Tester'
        }
      });
    });
    
    it('should extract config from markdown', async () => {
      const mockMarkdown = `# Configuration File
      
This is a markdown file with embedded configuration.

\`\`\`yaml
ai_protocol_version: '0.1'
identity:
  name: TestAssistant
  role: Tester
\`\`\`

Additional instructions here.
`;
      (fs.readFile as jest.Mock).mockResolvedValue(mockMarkdown);
      
      const config = await loadConfig('config.md');
      
      expect(fs.readFile).toHaveBeenCalledWith('config.md', 'utf-8');
      expect(config).toEqual({
        ai_protocol_version: '0.1',
        identity: {
          name: 'TestAssistant',
          role: 'Tester'
        }
      });
    });
    
    it('should throw error for invalid markdown format', async () => {
      const mockInvalidMarkdown = `# Configuration File
      
This is a markdown file with NO valid configuration.
`;
      (fs.readFile as jest.Mock).mockResolvedValue(mockInvalidMarkdown);
      
      await expect(loadConfig('config.md')).rejects.toThrow('No valid YAML code block found');
    });
  });
  
  describe('validateConfig', () => {
    it('should validate a valid configuration', () => {
      const config: AIConfig = {
        ai_protocol_version: '0.1',
        identity: {
          name: 'TestAssistant',
          role: 'Tester'
        },
        behavior: {
          voice: 'professional',
          autonomy: 'suggest'
        }
      };
      
      const result = validateConfig(config);
      
      expect(result.valid).toBe(true);
      expect(result.errors).toBeUndefined();
    });
    
    it('should fail validation for missing required fields', () => {
      const invalidConfig = {
        identity: {
          name: 'TestAssistant'
        }
      } as AIConfig;
      
      const result = validateConfig(invalidConfig);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toBeDefined();
      expect(result.errors?.length).toBeGreaterThan(0);
    });
    
    it('should detect conflicting capabilities', () => {
      const config: AIConfig = {
        ai_protocol_version: '0.1',
        identity: {
          name: 'TestAssistant',
          role: 'Tester'
        },
        capabilities: {
          allowed: ['code_review', 'deployment'],
          restricted: ['deployment', 'database_management']
        }
      };
      
      const result = validateConfig(config);
      
      expect(result.valid).toBe(true);
      expect(result.warnings).toBeDefined();
      expect(result.warnings).toContain(expect.stringContaining('deployment'));
    });
  });
  
  describe('mergeConfigs', () => {
    it('should merge multiple configurations', () => {
      const baseConfig: AIConfig = {
        ai_protocol_version: '0.1',
        identity: {
          name: 'BaseAssistant',
          role: 'Helper'
        },
        behavior: {
          voice: 'professional'
        },
        capabilities: {
          allowed: ['code_review', 'testing']
        }
      };
      
      const projectConfig: AIConfig = {
        ai_protocol_version: '0.1',
        identity: {
          name: 'ProjectAssistant'
        },
        behavior: {
          autonomy: 'suggest'
        },
        capabilities: {
          restricted: ['deployment']
        }
      };
      
      const merged = mergeConfigs([baseConfig, projectConfig]);
      
      expect(merged.identity?.name).toBe('ProjectAssistant');
      expect(merged.identity?.role).toBe('Helper');
      expect(merged.behavior?.voice).toBe('professional');
      expect(merged.behavior?.autonomy).toBe('suggest');
      expect(merged.capabilities?.allowed).toContain('code_review');
      expect(merged.capabilities?.restricted).toContain('deployment');
    });
    
    it('should handle empty array', () => {
      expect(() => mergeConfigs([])).toThrow('No configurations provided');
    });
    
    it('should return single config unchanged', () => {
      const config: AIConfig = {
        ai_protocol_version: '0.1',
        identity: {
          name: 'TestAssistant',
          role: 'Tester'
        }
      };
      
      const result = mergeConfigs([config]);
      
      expect(result).toEqual(config);
    });
    
    it('should remove allowed capabilities that are restricted', () => {
      const baseConfig: AIConfig = {
        ai_protocol_version: '0.1',
        capabilities: {
          allowed: ['code_review', 'deployment', 'testing']
        }
      };
      
      const projectConfig: AIConfig = {
        ai_protocol_version: '0.1',
        capabilities: {
          restricted: ['deployment']
        }
      };
      
      const merged = mergeConfigs([baseConfig, projectConfig]);
      
      expect(merged.capabilities?.allowed).toContain('code_review');
      expect(merged.capabilities?.allowed).toContain('testing');
      expect(merged.capabilities?.allowed).not.toContain('deployment');
      expect(merged.capabilities?.restricted).toContain('deployment');
    });
  });
});