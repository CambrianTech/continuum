import { initCommand } from '../../src/commands/init';
import { getTemplate, listTemplates } from '../../src/templates';
import { writeConfigFile } from '@continuum/core';
import inquirer from 'inquirer';
// Imports used for mock typing only

// Mocks
jest.mock('../../src/templates', () => ({
  getTemplate: jest.fn(),
  listTemplates: jest.fn(),
}));

jest.mock('@continuum/core', () => ({
  writeConfigFile: jest.fn(),
}));

jest.mock('inquirer', () => ({
  prompt: jest.fn()
}));
jest.mock('chalk', () => ({
  blue: jest.fn((text) => text),
  green: jest.fn((text) => text),
  yellow: jest.fn((text) => text),
  red: jest.fn((text) => text),
}));

// Mock console methods
const originalConsoleLog = console.log;
const originalConsoleError = console.error;
const mockExit = jest.spyOn(process, 'exit').mockImplementation((code) => {
  throw new Error(`Process.exit(${code})`);
});

describe('initCommand', () => {
  // Create placeholder for console mocks
  let consoleLogMock: jest.SpyInstance;
  let consoleErrorMock: jest.SpyInstance;
  
  beforeEach(() => {
    // Setup mocks
     
    consoleLogMock = jest.spyOn(console, 'log').mockImplementation(() => { /* empty for testing */ });
     
    consoleErrorMock = jest.spyOn(console, 'error').mockImplementation(() => { /* empty for testing */ });
    
    // Reset all mock data
    jest.clearAllMocks();

    // Mock template data
    const mockTemplate = {
      ai_protocol_version: '0.1',
      identity: {
        name: 'TestAssistant',
        role: 'Test role'
      },
      behavior: {
        voice: 'professional',
        autonomy: 'suggest',
        verbosity: 'concise'
      },
      capabilities: {
        allowed: ['testing'],
        restricted: ['deployment']
      }
    };
    
    (getTemplate as jest.Mock).mockResolvedValue(mockTemplate);
    (listTemplates as jest.Mock).mockResolvedValue(['standard', 'tdd', 'enterprise']);
    (writeConfigFile as jest.Mock).mockResolvedValue(undefined);
    
    // Provide empty function implementations for mock console functions
     
    jest.spyOn(console, 'log').mockImplementation(() => { /* empty for testing */ });
     
    jest.spyOn(console, 'error').mockImplementation(() => { /* empty for testing */ });
    
    // Mock inquirer responses
    (inquirer.prompt as unknown as jest.Mock).mockImplementation((questions) => {
      // Default answers based on question types
      const answers: Record<string, unknown> = {};
      
      for (const question of questions) {
        if (question.name === 'template') {
          answers.template = 'standard';
        } else if (question.name === 'name') {
          answers.name = 'TestAssistant';
        } else if (question.name === 'role') {
          answers.role = 'Test role';
        } else if (question.name === 'voice') {
          answers.voice = 'professional';
        } else if (question.name === 'autonomy') {
          answers.autonomy = 'suggest';
        } else if (question.name === 'verbosity') {
          answers.verbosity = 'concise';
        } else if (question.name === 'capabilities') {
          answers.capabilities = ['code_review', 'testing'];
        } else if (question.name === 'restricted') {
          answers.restricted = ['deployment'];
        }
      }
      
      return Promise.resolve(answers);
    });
  });
  
  afterEach(() => {
    // Restore console methods
    consoleLogMock.mockRestore();
    consoleErrorMock.mockRestore();
  });
  
  afterAll(() => {
    // Full restore
    mockExit.mockRestore();
    console.log = originalConsoleLog;
    console.error = originalConsoleError;
  });

  it('should initialize configuration with default options', async () => {
    // Run command
    await initCommand({ template: 'standard', output: 'test-config.md' });
    
    // Verify template was loaded
    expect(getTemplate).toHaveBeenCalledWith('standard');
    
    // Verify config was written
    expect(writeConfigFile).toHaveBeenCalled();
    expect((writeConfigFile as unknown as jest.Mock).mock.calls[0][1]).toContain('test-config.md');
    
    // Verify console output
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Initializing new AI configuration'));
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Configuration successfully created'));
  });
  
  it('should prompt for template if not provided', async () => {
    // Run command without template
    await initCommand({ template: '', output: 'test-config.md' });
    
    // Verify templates were listed
    expect(listTemplates).toHaveBeenCalled();
    
    // Verify template selection prompt was shown
    expect(inquirer.prompt).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'list',
          name: 'template'
        })
      ])
    );
  });
  
  it('should handle errors properly', async () => {
    // Setup error
    (getTemplate as jest.Mock).mockRejectedValue(new Error('Test error'));
    
    // Verify error handling
    await expect(initCommand({ template: 'standard', output: 'test-config.md' }))
      .rejects.toThrow('Process.exit(1)');
    
    expect(consoleErrorMock).toHaveBeenCalledWith(
      expect.stringContaining('Error initializing configuration: Error: Test error')
    );
  });
});