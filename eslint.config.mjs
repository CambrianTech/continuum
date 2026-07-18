// Clippy-grade lint gate for the NEW client tree — the SDK + shared views + the
// apps built on them. Legacy `src/` keeps its own `src/eslint.config.js`; this
// config deliberately does NOT touch it (that monolith is reference-only). Here
// the bar is: type-checked, strict, zero `any`, and it BLOCKS — `--max-warnings 0`
// means a warning is a failure, the way `cargo clippy -D warnings` won't let a
// Rust change through with a lint outstanding.
//
// Scope: packages/**, apps/web, apps/tui, sdk/typescript — the hand-written
// client code this gate owns. The SDK is now folded in: the ts-rs u64/i64→bigint
// drift on the contract payloads is fixed at the Rust source (`#[ts(type =
// "number")]`, the CLAUDE.md-canonical mapping), so the vendored wire types are
// `number` and the SDK typechecks + lints clean. Generated bindings
// (`sdk/typescript/generated/**`) stay excluded — that is machine output carrying
// a `// Do not edit` header; we lint hand-written source, not codegen.

import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import globals from 'globals';

export default tseslint.config(
  {
    // Only the new client tree. Everything else (legacy src/, tooling js, build
    // output, generated bindings) is out of scope for this gate.
    ignores: [
      '**/node_modules/**',
      '**/dist/**',
      'src/**',
      'core/**',
      'tools/**',
      'scripts/**',
      // Machine-generated ts-rs bindings (vendored wire + views + positron) carry
      // a `// Do not edit` header — typechecked (they're in the SDK tsconfig) but
      // never linted. Hand-written SDK source is linted (see files glob below).
      'sdk/typescript/generated/**',
      '**/*.js',
      '**/*.mjs',
      '**/*.cjs',
    ],
  },

  // Type-checked strict base — the real "clippy": rules that need the type
  // system to fire (no-floating-promises, no-unsafe-*, etc.).
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,

  {
    files: ['packages/**/*.ts', 'apps/web/**/*.ts', 'apps/tui/**/*.ts', 'sdk/typescript/**/*.ts'],
    languageOptions: {
      // projectService auto-resolves each file to its nearest tsconfig, so one
      // config type-checks four packages with four different lib/target sets.
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // Zero tolerance for the escape hatches the doctrine forbids.
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-non-null-assertion': 'error',
      // Promise safety — an unawaited send/connect is a real bug in this code.
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      // Every exported/public function states its return type (intent over
      // inference at the boundary).
      '@typescript-eslint/explicit-function-return-type': [
        'error',
        { allowExpressions: true, allowTypedFunctionExpressions: true },
      ],
      '@typescript-eslint/consistent-type-imports': [
        'error',
        { prefer: 'type-imports', fixStyle: 'inline-type-imports' },
      ],
      // Numbers in template literals are type-safe and idiomatic (member counts,
      // timestamps). Everything else the strict default forbids stays forbidden.
      '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
    },
  },

  {
    // Test files run under Node (vitest globals come from the import, but the
    // process/env access in specs is Node). Production code keeps the FULL strict
    // gate; specs relax three rules that are ergonomic-only in test code and whose
    // strictness buys nothing there: `!` on known-present fixtures, no-op mock
    // callbacks (`() => {}` unsubscribe stubs), and return types on inline test
    // helpers. The safety rules (no-explicit-any, no-floating-promises, no-unsafe-*)
    // stay ON in specs.
    files: ['**/*.spec.ts', '**/*.test.ts'],
    languageOptions: {
      globals: { ...globals.node },
    },
    rules: {
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-empty-function': 'off',
      '@typescript-eslint/explicit-function-return-type': 'off',
    },
  },
);
