import { validateCommand } from '../../src/commands/validate';
import { validateConfig, loadConfig } from '@continuum/core';
import chalk from 'chalk';
import * as fs from 'fs/promises';
import * as path from 'path';

// Mocks
jest.mock('@continuum/core', () => ({
  validateConfig: jest.fn(),
  loadConfig: jest.fn(),
}));

jest.mock('fs/promises', () => ({
  access: jest.fn(),
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

describe('validateCommand', () => {
  // Create placeholder for console mocks
  let consoleLogMock: jest.SpyInstance;
  let consoleErrorMock: jest.SpyInstance;
  
  beforeEach(() => {
    // Setup mocks
    consoleLogMock = jest.spyOn(console, 'log').mockImplementation(() => {});
    consoleErrorMock = jest.spyOn(console, 'error').mockImplementation(() => {});
    
    // Reset all mock data
    jest.clearAllMocks();
    
    // Default mock responses
    (fs.access as jest.Mock).mockResolvedValue(undefined);
    
    // Default config
    const mockConfig = {
      ai_protocol_version: '0.1',
      identity: {
        name: 'TestAssistant',
        role: 'Test role'
      },
      behavior: {
        voice: 'professional',
        autonomy: 'suggest'
      },
      capabilities: {
        allowed: ['code_review', 'testing'],
        restricted: ['deployment']
      }
    };
    
    (loadConfig as jest.Mock).mockResolvedValue(mockConfig);
    (validateConfig as jest.Mock).mockReturnValue({ valid: true });
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

  it('should successfully validate a valid configuration', async () => {
    await validateCommand({ config: 'test-config.md' });
    
    // Verify file was checked
    expect(fs.access).toHaveBeenCalled();
    
    // Verify config was loaded and validated
    expect(loadConfig).toHaveBeenCalled();
    expect(validateConfig).toHaveBeenCalled();
    
    // Verify success message
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Configuration is valid'));
  });
  
  it('should display configuration summary', async () => {
    await validateCommand({ config: 'test-config.md' });
    
    // Verify summary was displayed
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Configuration Summary'));
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Assistant Name: TestAssistant'));
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Role: Test role'));
  });
  
  it('should display warnings if present', async () => {
    // Setup warning
    (validateConfig as jest.Mock).mockReturnValue({
      valid: true,
      warnings: ['Missing recommended field']
    });
    
    await validateCommand({ config: 'test-config.md' });
    
    // Verify warnings were displayed
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Warnings'));
    expect(consoleLogMock).toHaveBeenCalledWith(expect.stringContaining('Missing recommended field'));
  });
  
  it('should handle invalid configuration', async () => {
    // Setup invalid config
    (validateConfig as jest.Mock).mockReturnValue({
      valid: false,
      errors: ['Required field missing']
    });
    
    // Expect exit with error
    await expect(validateCommand({ config: 'test-config.md' }))
      .rejects.toThrow('Process.exit(1)');
    
    // Verify error message
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('Configuration is invalid'));
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('Errors'));
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('Required field missing'));
  });
  
  it('should handle file not found', async () => {
    // Setup file not found
    (fs.access as jest.Mock).mockRejectedValue(new Error('File not found'));
    
    // Expect exit with error
    await expect(validateCommand({ config: 'test-config.md' }))
      .rejects.toThrow('Process.exit(1)');
    
    // Verify error message
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('Configuration file not found'));
  });
  
  it('should handle loading errors', async () => {
    // Setup loading error
    (loadConfig as jest.Mock).mockRejectedValue(new Error('Invalid YAML'));
    
    // Expect exit with error
    await expect(validateCommand({ config: 'test-config.md' }))
      .rejects.toThrow('Process.exit(1)');
    
    // Verify error message
    expect(consoleErrorMock).toHaveBeenCalledWith(expect.stringContaining('Error validating configuration'));
  });
});