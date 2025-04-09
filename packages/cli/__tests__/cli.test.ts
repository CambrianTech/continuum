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