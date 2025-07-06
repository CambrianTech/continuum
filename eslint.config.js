import tseslint from '@typescript-eslint/eslint-plugin';
import tsParser from '@typescript-eslint/parser';
import eslintJs from '@eslint/js';

export default [
  // Base config for all files
  {
    files: ['**/*.{js,ts}'],
    ignores: [
      'dist/**',
      'node_modules/**',
      'coverage/**',
      '*.config.js',
      '**/dist/**',
      '.continuum/**',
      'python-client/.venv/**',
      'python-client/**/*.log',
      'test_screenshots/**',
      'agents/workspace/**',
      '**/venv/**',
      '**/env/**',
      '**/htmlfiles/**'
    ],
    languageOptions: {
      parser: tsParser,
      ecmaVersion: 2022,
      sourceType: 'module',
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module'
      },
      globals: {
        // Node.js globals
        process: 'readonly',
        console: 'readonly',
        module: 'readonly',
        require: 'readonly',
        __dirname: 'readonly',
        __filename: 'readonly',
        setTimeout: 'readonly',
        setInterval: 'readonly',
        setImmediate: 'readonly',
        clearTimeout: 'readonly',
        clearInterval: 'readonly',
        Buffer: 'readonly',
        global: 'readonly',
      }
    },
    plugins: {
      '@typescript-eslint': tseslint
    },
    rules: {
      ...eslintJs.configs.recommended.rules,
      ...tseslint.configs.recommended.rules,
      '@typescript-eslint/no-explicit-any': 'error', // Changed from 'warn' to 'error'
      '@typescript-eslint/no-unused-vars': ['warn', { 'argsIgnorePattern': '^_' }],
      '@typescript-eslint/explicit-module-boundary-types': 'off',
      '@typescript-eslint/no-require-imports': 'error', // Changed from 'off' to 'error'
      'no-undef': 'error',
      // Custom rules for proper module imports
      'no-restricted-imports': ['error', {
        'patterns': ['*.js', '*.jsx', '*.ts', '*.tsx'] // No file extensions in imports
      }],
    }
  },
  
  // Config specifically for test files
  {
    files: ['**/__tests__/**/*.{js,ts}', '**/*.test.{js,ts}', '**/*.spec.{js,ts}'],
    plugins: {
      '@typescript-eslint': tseslint
    },
    languageOptions: {
      globals: {
        // Jest globals
        describe: 'readonly',
        it: 'readonly',
        test: 'readonly',
        expect: 'readonly',
        beforeEach: 'readonly',
        afterEach: 'readonly',
        beforeAll: 'readonly',
        afterAll: 'readonly',
        jest: 'readonly',
        global: 'readonly',
        Buffer: 'readonly',
        setTimeout: 'readonly',
      }
    }
  },

  // Config for browser-side scripts (agent-scripts, UI components)
  {
    files: ['agent-scripts/**/*.js', 'src/ui/**/*.js', '**/browser*.js', 'src/modules/**/*.js'],
    languageOptions: {
      globals: {
        // Browser globals
        window: 'readonly',
        document: 'readonly',
        navigator: 'readonly',
        location: 'readonly',
        alert: 'readonly',
        console: 'readonly',
        setTimeout: 'readonly',
        setInterval: 'readonly',
        clearTimeout: 'readonly',
        clearInterval: 'readonly',
        requestAnimationFrame: 'readonly',
        WebSocket: 'readonly',
        Response: 'readonly',
        performance: 'readonly',
        Buffer: 'readonly',
        fetch: 'readonly',
        URL: 'readonly',
        CustomEvent: 'readonly',
        HTMLElement: 'readonly',
        customElements: 'readonly',
        localStorage: 'readonly',
        confirm: 'readonly',
        define: 'readonly',
        ws: 'writable',
        addMessage: 'readonly',
        addSystemMessage: 'readonly',
        initWebSocket: 'readonly',
        handleWebSocketMessage: 'readonly',
        BaseWidget: 'readonly',
        SidebarWidget: 'readonly',
        captureWidgetScreenshot: 'readonly',
        validateScreenshotContent: 'readonly',
        runSelfDiagnostics: 'readonly',
        commands: 'readonly',
      }
    }
  },

  // Config for archived/experimental files (more lenient)
  {
    files: ['archive/**/*.{js,ts}', 'archived/**/*.{js,ts}', 'examples/**/*.js', 'agent-scripts/**/*.js'],
    plugins: {
      '@typescript-eslint': tseslint
    },
    rules: {
      '@typescript-eslint/no-unused-vars': 'warn',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-expressions': 'warn',
      'no-undef': 'warn',
      'no-global-assign': 'warn',
      'no-prototype-builtins': 'warn',
    }
  },

  // Config for CommonJS files
  {
    files: ['**/*.cjs'],
    languageOptions: {
      sourceType: 'script',
      ecmaVersion: 2022,
    },
    rules: {
      '@typescript-eslint/no-require-imports': 'off',
    }
  }
];