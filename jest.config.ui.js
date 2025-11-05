const config = {
  preset: 'ts-jest',
  testEnvironment: 'jsdom',
  roots: ['<rootDir>/tests/ui', '<rootDir>/tests/communication', '<rootDir>/tests/integration'],
  testMatch: ['**/*.test.ts'],
  collectCoverageFrom: [
    'src/ui/**/*.ts',
    '!src/ui/**/*.d.ts'
  ],
  coveragePathIgnorePatterns: [
    '/node_modules/',
    '/packages/',
    '/dist/'
  ],
  transform: {
    '^.+\\.ts$': ['ts-jest', {
      isolatedModules: true,
      tsconfig: {
        module: 'commonjs',
        target: 'es2018',
        noImplicitAny: false,
        strict: false
      }
    }]
  },
  coverageDirectory: 'coverage/ui',
  coverageReporters: ['text', 'lcov', 'html'],
  verbose: true,
  testTimeout: 10000
};

module.exports = config;