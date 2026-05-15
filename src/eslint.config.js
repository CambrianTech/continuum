// @ts-check
import tseslint from 'typescript-eslint';

export default tseslint.config(
  // Base recommended rules
  ...tseslint.configs.recommended,

  // Project-specific configuration
  {
    languageOptions: {
      parserOptions: {
        project: ['./tsconfig.eslint.json', './tsconfig.eslint.precommit.json'],
      },
    },
    rules: {
      // Type safety — zero tolerance for any
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': 'error',

      // Promise safety
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/require-await': 'warn',

      // Code quality
      '@typescript-eslint/prefer-nullish-coalescing': 'error',
      '@typescript-eslint/explicit-function-return-type': 'warn',

      // Complexity limits
      'max-lines': ['error', { max: 500, skipBlankLines: true, skipComments: true }],
      'max-lines-per-function': ['error', { max: 80, skipBlankLines: true, skipComments: true }],
      'complexity': ['error', 15],
      'max-depth': ['error', 4],
      'max-params': ['error', 5],
    },
  },

  // Ignore patterns
  {
    ignores: [
      'dist/**',
      'node_modules/**',
      'shared/config.ts',
      'shared/generated/**',
      'workers/target/**',
      'workers/vendor/**',
      '**/*.d.ts',
      '**/*.js',
      '**/*.mjs',
      '**/test/**/*.ts',
      'examples/**',
      'scripts/**',
      'generated-command-schemas.json',
    ],
  },
);
