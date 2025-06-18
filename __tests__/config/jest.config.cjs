const path = require('path');

module.exports = {
  // Root directory
  rootDir: path.resolve(__dirname, '../..'),
  
  // Test directories - includes both centralized and co-located modular tests
  testMatch: [
    '<rootDir>/__tests__/**/*.test.{js,ts,cjs}',
    '<rootDir>/__tests__/**/*.spec.{js,ts,cjs}',
    '<rootDir>/src/**/*.test.{js,ts,cjs}',
    '<rootDir>/src/**/*.spec.{js,ts,cjs}'
  ],
  
  // Module resolution
  moduleNameMapper: {
    '^@/(.*)$': '<rootDir>/src/$1',
    '^@tests/(.*)$': '<rootDir>/__tests__/$1'
  },
  
  // Environment setup
  testEnvironment: 'node',
  setupFilesAfterEnv: ['<rootDir>/__tests__/setup.js'],
  
  // Transform configuration
  transform: {
    '^.+\\.(js|jsx|ts|tsx)$': 'babel-jest',
    '^.+\\.cjs$': 'babel-jest'
  },
  
  // File extensions
  moduleFileExtensions: ['js', 'ts', 'cjs', 'json'],
  
  // Coverage configuration
  collectCoverage: false,
  coverageDirectory: '<rootDir>/coverage/js',
  coverageReporters: ['text', 'lcov', 'html'],
  collectCoverageFrom: [
    'src/**/*.{js,ts,cjs}',
    '!src/**/*.d.ts',
    '!src/**/*.test.{js,ts,cjs}',
    '!src/**/node_modules/**',
    '!coverage/**'
  ],
  
  // Test timeout
  testTimeout: 30000,
  
  // Verbose output
  verbose: true,
  
  // Reporter configuration
  reporters: [
    'default'
  ],
  
  // Test organization
  displayName: {
    name: 'Continuum JS Tests',
    color: 'blue'
  },
  
  // Global setup
  globalSetup: '<rootDir>/__tests__/config/jest.global-setup.js',
  globalTeardown: '<rootDir>/__tests__/config/jest.global-teardown.js',
  
  // Ignore patterns
  testPathIgnorePatterns: [
    '/node_modules/',
    '/coverage/',
    '/dist/',
    '/python-client/',
    '__tests__/fixtures/',
    '__tests__/config/',
    '/.continuum-safe-backup/',
    '/externals/'
  ],
  
  // Watch mode
  watchPathIgnorePatterns: [
    '/node_modules/',
    '/coverage/',
    '/dist/',
    '/.continuum-safe-backup/',
    '/externals/',
    '/python-client/'
  ]
};