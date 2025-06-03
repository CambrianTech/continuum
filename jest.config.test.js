/** @type {import('jest').Config} */
export default {
  testEnvironment: 'node',
  collectCoverage: true,
  coverageDirectory: 'coverage',
  testMatch: [
    '**/__tests__/**/*.test.js',
    '**/?(*.)+(spec|test).js'
  ],
  collectCoverageFrom: [
    'src/**/*.{js,cjs}',
    '!src/**/*.test.{js,cjs}',
    '!**/node_modules/**',
    '!**/coverage/**',
  ],
  coverageReporters: [
    'text',
    'lcov',
    'html'
  ],
  setupFilesAfterEnv: ['<rootDir>/__tests__/setup.js'],
  preset: 'ts-jest/presets/js-with-ts-esm',
  extensionsToTreatAsEsm: ['.js'],
  transform: {
    '^.+\\.m?js$': 'babel-jest',
  },
  testTimeout: 10000,
  verbose: true
};