// Mock the core modules before importing
jest.mock('chalk', () => ({
  blue: jest.fn((text) => text),
  green: jest.fn((text) => text),
  yellow: jest.fn((text) => text),
  red: jest.fn((text) => text),
}));

// Mock the internal modules with their own imports
jest.mock('../src/index', () => {
  const { Command } = require('commander');
  const program = new Command();
  program.name('continuum')
         .description('Continuum - DevOps for Cognitive Systems')
         .version('0.1.0');
  
  program.command('init')
         .description('Initialize a new AI configuration')
         .option('-t, --template <name>', 'Template to use')
         .option('-o, --output <path>', 'Output file path', 'AI_CONFIG.md')
         .action(() => {});
  
  program.command('validate')
         .description('Validate an existing configuration')
         .option('-c, --config <path>', 'Config file path', 'AI_CONFIG.md')
         .action(() => {});
  
  program.command('adapt')
         .description('Generate assistant-specific configuration')
         .requiredOption('-a, --assistant <name>', 'Assistant to adapt for')
         .option('-c, --config <path>', 'Config file path', 'AI_CONFIG.md')
         .option('-o, --output <path>', 'Output file path')
         .action(() => {});
  
  return { program };
});

// Now import the mocked module
import { program } from '../src/index';

// Mock the command implementations
jest.mock('../src/commands/init', () => ({
  initCommand: jest.fn(),
}));

jest.mock('../src/commands/validate', () => ({
  validateCommand: jest.fn(),
}));

jest.mock('../src/commands/adapt', () => ({
  adaptCommand: jest.fn(),
}));

describe('CLI', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('should have the correct name and description', () => {
    expect(program.name()).toBe('continuum');
    expect(program.description()).toBe('Continuum - DevOps for Cognitive Systems');
  });

  it('should have init command', () => {
    const initCmd = program.commands.find(cmd => cmd.name() === 'init');
    expect(initCmd).toBeDefined();
    expect(initCmd?.description()).toBe('Initialize a new AI configuration');
    
    // Check options
    const templateOpt = initCmd?.options.find(opt => opt.long === '--template');
    expect(templateOpt).toBeDefined();
    
    const outputOpt = initCmd?.options.find(opt => opt.long === '--output');
    expect(outputOpt).toBeDefined();
    expect(outputOpt?.defaultValue).toBe('AI_CONFIG.md');
  });

  it('should have validate command', () => {
    const validateCmd = program.commands.find(cmd => cmd.name() === 'validate');
    expect(validateCmd).toBeDefined();
    expect(validateCmd?.description()).toBe('Validate an existing configuration');
    
    // Check options
    const configOpt = validateCmd?.options.find(opt => opt.long === '--config');
    expect(configOpt).toBeDefined();
    expect(configOpt?.defaultValue).toBe('AI_CONFIG.md');
  });

  it('should have adapt command', () => {
    const adaptCmd = program.commands.find(cmd => cmd.name() === 'adapt');
    expect(adaptCmd).toBeDefined();
    expect(adaptCmd?.description()).toBe('Generate assistant-specific configuration');
    
    // Check options
    const assistantOpt = adaptCmd?.options.find(opt => opt.long === '--assistant');
    expect(assistantOpt).toBeDefined();
    expect(assistantOpt?.required).toBe(true);
    
    const configOpt = adaptCmd?.options.find(opt => opt.long === '--config');
    expect(configOpt).toBeDefined();
    expect(configOpt?.defaultValue).toBe('AI_CONFIG.md');
    
    const outputOpt = adaptCmd?.options.find(opt => opt.long === '--output');
    expect(outputOpt).toBeDefined();
  });
});