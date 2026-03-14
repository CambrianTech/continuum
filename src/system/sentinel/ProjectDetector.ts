/**
 * ProjectDetector — Detects build system and test framework for a project directory.
 *
 * Used by dev templates to auto-configure buildCommand and testCommand
 * when the user doesn't specify them. Supports:
 *   - Node.js (package.json → npm)
 *   - Rust (Cargo.toml → cargo)
 *   - Python (pyproject.toml/setup.py → pytest/python)
 *   - Go (go.mod → go)
 *   - Java/Kotlin (pom.xml → maven, build.gradle → gradle)
 *
 * Returns null for commands that should be skipped (e.g., no test runner found).
 */

import * as fs from 'fs';
import * as path from 'path';

export interface ProjectInfo {
  /** Detected project type */
  type: 'node' | 'rust' | 'python' | 'go' | 'java-maven' | 'java-gradle' | 'unknown';
  /** Build command (null = skip build step) */
  buildCommand: string | null;
  /** Test command (null = skip test step) */
  testCommand: string | null;
  /** Whether a CLAUDE.md exists in the project */
  hasClaudeMd: boolean;
}

/**
 * Detect project type and build/test commands for a directory.
 */
export function detectProject(cwd: string): ProjectInfo {
  const exists = (file: string) => fs.existsSync(path.join(cwd, file));
  const hasClaudeMd = exists('CLAUDE.md');

  // Node.js
  if (exists('package.json')) {
    const buildCommand = detectNodeBuildCommand(cwd);
    const testCommand = detectNodeTestCommand(cwd);
    return { type: 'node', buildCommand, testCommand, hasClaudeMd };
  }

  // Rust
  if (exists('Cargo.toml')) {
    return {
      type: 'rust',
      buildCommand: 'cargo build 2>&1 | tail -30',
      testCommand: 'cargo test 2>&1 | tail -50',
      hasClaudeMd,
    };
  }

  // Python
  if (exists('pyproject.toml') || exists('setup.py') || exists('setup.cfg')) {
    const testCommand = exists('pytest.ini') || exists('pyproject.toml')
      ? 'pytest 2>&1 | tail -50'
      : 'python -m pytest 2>&1 | tail -50';
    return {
      type: 'python',
      buildCommand: null, // Python doesn't typically have a build step
      testCommand,
      hasClaudeMd,
    };
  }

  // Go
  if (exists('go.mod')) {
    return {
      type: 'go',
      buildCommand: 'go build ./... 2>&1 | tail -30',
      testCommand: 'go test ./... 2>&1 | tail -50',
      hasClaudeMd,
    };
  }

  // Java/Kotlin (Maven)
  if (exists('pom.xml')) {
    return {
      type: 'java-maven',
      buildCommand: 'mvn compile 2>&1 | tail -30',
      testCommand: 'mvn test 2>&1 | tail -50',
      hasClaudeMd,
    };
  }

  // Java/Kotlin (Gradle)
  if (exists('build.gradle') || exists('build.gradle.kts')) {
    return {
      type: 'java-gradle',
      buildCommand: './gradlew build 2>&1 | tail -30',
      testCommand: './gradlew test 2>&1 | tail -50',
      hasClaudeMd,
    };
  }

  return {
    type: 'unknown',
    buildCommand: null,
    testCommand: null,
    hasClaudeMd,
  };
}

/**
 * Detect the best build command for a Node.js project.
 */
function detectNodeBuildCommand(cwd: string): string | null {
  try {
    const pkg = JSON.parse(fs.readFileSync(path.join(cwd, 'package.json'), 'utf-8'));
    const scripts = pkg.scripts || {};

    // Prefer build:ts (TypeScript-specific), then build
    if (scripts['build:ts']) return 'npm run build:ts 2>&1 | tail -30';
    if (scripts['build']) return 'npm run build 2>&1 | tail -30';
    if (scripts['compile']) return 'npm run compile 2>&1 | tail -30';

    // Check for TypeScript config (compile without explicit build script)
    if (fs.existsSync(path.join(cwd, 'tsconfig.json'))) {
      return 'npx tsc --noEmit 2>&1 | tail -30';
    }

    return null; // No build step needed
  } catch {
    return null;
  }
}

/**
 * Detect the best test command for a Node.js project.
 */
function detectNodeTestCommand(cwd: string): string | null {
  try {
    const pkg = JSON.parse(fs.readFileSync(path.join(cwd, 'package.json'), 'utf-8'));
    const scripts = pkg.scripts || {};

    if (scripts['test']) return 'npm test 2>&1 | tail -50';
    if (scripts['test:unit']) return 'npm run test:unit 2>&1 | tail -50';

    // Check for common test runners
    const deps = { ...pkg.devDependencies, ...pkg.dependencies };
    if (deps['vitest']) return 'npx vitest run 2>&1 | tail -50';
    if (deps['jest']) return 'npx jest 2>&1 | tail -50';
    if (deps['mocha']) return 'npx mocha 2>&1 | tail -50';

    return null; // No test runner found
  } catch {
    return null;
  }
}
