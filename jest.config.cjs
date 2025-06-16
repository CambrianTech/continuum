/** @type {import('jest').Config} */
module.exports = {
  testEnvironment: 'node',
  collectCoverage: true,
  coverageDirectory: 'coverage',
  transform: {
    '^.+\\.js$': 'babel-jest',
    '^.+\\.cjs$': 'babel-jest',
    '^.+\\.ts$': 'ts-jest'
  },
  testMatch: [
    '**/__tests__/**/*.test.{js,cjs,ts}',
    '**/?(*.)+(spec|test).{js,cjs,ts}'
  ],
  modulePathIgnorePatterns: [
    '<rootDir>/dist/',
    '<rootDir>/node_modules/',
    '<rootDir>/.continuum-safe-backup/'
  ],
  transformIgnorePatterns: [
    'node_modules/(?!(chalk|inquirer|commander)/)'
  ],
  setupFilesAfterEnv: [],
  testTimeout: 30000,
  collectCoverageFrom: [
    'src/**/*.{js,cjs,ts}',
    '!src/**/*.test.{js,cjs,ts}',
    '!**/node_modules/**',
    '!**/dist/**',
    '!**/coverage/**'
  ]
};