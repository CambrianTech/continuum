import { adaptCommand } from '../../src/commands/adapt';
import { loadConfig } from '@continuum/core';
import { getAdapter } from '../../src/adapters';
import * as fs from 'fs/promises';

// Mocks
jest.mock('@continuum/core', () => ({
  loadConfig: jest.fn(),
}));

jest.mock('../../src/adapters', () => ({
  getAdapter: jest.fn(),
}));

jest.mock('fs/promises', () => ({
  access: jest.fn(),
  writeFile: jest.fn().mockResolvedValue(undefined),
  unlink: jest.fn().mockResolvedValue(undefined),
  symlink: jest.fn().mockResolvedValue(undefined),
}));

jest.mock('chalk', () => ({
  blue: jest.fn((text) => text),
  green: jest.fn((text) => text),
  yellow: jest.fn((text) => text),
  red: jest.fn((text) => text),
}));

// Mock process.exit
const mockExit = jest.spyOn(process, 'exit').mockImplementation((code) => {
  throw new Error(`Process.exit(${code})`);
});

// Mock console methods
const originalConsoleLog = console.log;
const originalConsoleError = console.error;

describe('adaptCommand', () => {
  // Create placeholder for console mocks
  let consoleLogMock: jest.SpyInstance;
  let consoleErrorMock: jest.SpyInstance;
  
  beforeEach(() => {
    // Setup mocks
    consoleLogMock = jest.spyOn(console, 'log').mockImplementation(() => { /* empty for testing */ });
    consoleErrorMock = jest.spyOn(console, 'error').mockImplementation(() => { /* empty for testing */ });
    
    // Reset all mock data
    jest.clearAllMocks();
    
    // Default mock responses
    (fs.access as jest.Mock).mockResolvedValue(undefined);
    (fs.writeFile as jest.Mock).mockResolvedValue(undefined);
    
    // Default config
    const mockConfig = {
      ai_protocol_version: '0.1',
      identity: {
        name: 'TestAssistant',
        role: 'Test role'
      }
    };
    
    // Mock adapter
    const mockAdapter = {
      name: 'claude',
      formatForAssistant: jest.fn().mockReturnValue('# Claude System Prompt\nYou are TestAssistant'),
      loadConfig: jest.fn(),
      mergeConfigs: jest.fn()
    };
    
    (loadConfig as jest.Mock).mockResolvedValue(mockConfig);
    (getAdapter as jest.Mock).mockReturnValue(mockAdapter);
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

  it('should successfully adapt configuration for a supported assistant', async () => {
    await adaptCommand({
      assistant: 'claude',
      config: '.continuum/default/config.md',
      output: '.continuum/claude/config.md'
    });
    
    // Verify file was checked
    expect(fs.access).toHaveBeenCalled();
    
    // Verify adapter was fetched
    expect(getAdapter).toHaveBeenCalledWith('claude');
    
    // Verify config was loaded
    expect(loadConfig).toHaveBeenCalled();
    
    // Verify file was written
    expect(fs.writeFile).toHaveBeenCalled();
    expect((fs.writeFile as unknown as jest.Mock).mock.calls[0][0]).toContain('.continuum/claude/config.md');
    expect((fs.writeFile as unknown as jest.Mock).mock.calls[0][1]).toContain('Claude System Prompt');
    
    // Verify success message
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Adapted configuration for claude created at'));
  });
  
  it('should use default output name when no output is provided', async () => {
    await adaptCommand({
      assistant: 'claude',
      config: '.continuum/default/config.md'
    });
    
    // Verify default output was used
    expect((fs.writeFile as unknown as jest.Mock).mock.calls[0][0]).toContain('.continuum/claude/config.md');
  });
  
  it('should create symlink when createLink option is true', async () => {
    await adaptCommand({
      assistant: 'claude',
      config: '.continuum/default/config.md',
      createLink: true
    });
    
    // Verify symlink was created
    expect(fs.symlink).toHaveBeenCalled();
    expect((fs.symlink as unknown as jest.Mock).mock.calls[0][1]).toContain('CLAUDE.md');
  });
  
  it('should handle unsupported assistants', async () => {
    // Setup adapter not found
    (getAdapter as jest.Mock).mockReturnValue(undefined);
    
    // Expect exit with error
    await expect(adaptCommand({
      assistant: 'unsupported',
      config: '.continuum/default/config.md'
    })).rejects.toThrow('Process.exit(1)');
    
    // Verify error message
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('No adapter found for assistant'));
  });
  
  it('should handle file not found', async () => {
    // Setup file not found
    (fs.access as jest.Mock).mockRejectedValue(new Error('File not found'));
    
    // Expect exit with error
    await expect(adaptCommand({
      assistant: 'claude',
      config: '.continuum/missing/config.md'
    })).rejects.toThrow('Process.exit(1)');
    
    // Verify error message
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('Configuration file not found'));
  });
  
  it('should handle adapter formatting errors', async () => {
    // Setup adapter error
    const mockAdapter = {
      name: 'claude',
      formatForAssistant: jest.fn().mockImplementation(() => {
        throw new Error('Formatting error');
      }),
      loadConfig: jest.fn(),
      mergeConfigs: jest.fn()
    };
    (getAdapter as jest.Mock).mockReturnValue(mockAdapter);
    
    // Expect exit with error
    await expect(adaptCommand({
      assistant: 'claude',
      config: '.continuum/default/config.md'
    })).rejects.toThrow('Process.exit(1)');
    
    // Verify error message
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('Error adapting configuration'));
  });
});