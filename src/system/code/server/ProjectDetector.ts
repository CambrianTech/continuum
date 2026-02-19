/**
 * ProjectDetector - Detect project type from workspace contents
 *
 * Examines root-level files to determine the project ecosystem
 * and infer build/test/serve commands. Used by:
 * - ProjectContextSource (RAG): surfaces project type and commands in context
 * - Workspace.detectProjectType(): convenience for tooling
 *
 * Intentionally simple: file existence checks + basic JSON/TOML parsing.
 */

import * as fs from 'fs';
import * as path from 'path';

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

export type ProjectTypeName = 'node' | 'rust' | 'python' | 'xcode' | 'go' | 'swift-package' | 'unknown';

export interface ProjectType {
  /** Detected project ecosystem */
  readonly type: ProjectTypeName;

  /** Primary build command (e.g., 'npm run build', 'cargo build') */
  readonly buildCommand?: string;

  /** Primary test command (e.g., 'npm test', 'cargo test') */
  readonly testCommand?: string;

  /** Dev server / run command (e.g., 'npm run dev', 'cargo run') */
  readonly serveCommand?: string;

  /** Lock file path (e.g., 'package-lock.json', 'Cargo.lock') */
  readonly lockFile?: string;

  /** Primary config file (e.g., 'package.json', 'Cargo.toml') */
  readonly entryFile?: string;

  /** Human-readable description */
  readonly description: string;
}

// ────────────────────────────────────────────────────────────
// Detector
// ────────────────────────────────────────────────────────────

export class ProjectDetector {

  /**
   * Detect project type by examining files in `dir`.
   * Checks in priority order — first match wins.
   */
  static async detect(dir: string): Promise<ProjectType> {
    // Rust (Cargo.toml)
    if (fs.existsSync(path.join(dir, 'Cargo.toml'))) {
      return this.detectRust(dir);
    }

    // Node.js (package.json)
    if (fs.existsSync(path.join(dir, 'package.json'))) {
      return this.detectNode(dir);
    }

    // Go (go.mod)
    if (fs.existsSync(path.join(dir, 'go.mod'))) {
      return {
        type: 'go',
        buildCommand: 'go build ./...',
        testCommand: 'go test ./...',
        serveCommand: 'go run .',
        lockFile: fs.existsSync(path.join(dir, 'go.sum')) ? 'go.sum' : undefined,
        entryFile: 'go.mod',
        description: 'Go module',
      };
    }

    // Python (pyproject.toml or setup.py)
    if (fs.existsSync(path.join(dir, 'pyproject.toml'))) {
      return this.detectPython(dir);
    }
    if (fs.existsSync(path.join(dir, 'setup.py'))) {
      return {
        type: 'python',
        buildCommand: 'python setup.py build',
        testCommand: 'python -m pytest',
        entryFile: 'setup.py',
        description: 'Python package (setup.py)',
      };
    }

    // Xcode (*.xcodeproj or *.xcworkspace)
    const xcodeProject = this.findXcodeProject(dir);
    if (xcodeProject) {
      return {
        type: 'xcode',
        buildCommand: `xcodebuild -project "${xcodeProject}" build`,
        testCommand: `xcodebuild -project "${xcodeProject}" test`,
        entryFile: xcodeProject,
        description: `Xcode project (${xcodeProject})`,
      };
    }

    // Swift Package (Package.swift without .xcodeproj)
    if (fs.existsSync(path.join(dir, 'Package.swift'))) {
      return {
        type: 'swift-package',
        buildCommand: 'swift build',
        testCommand: 'swift test',
        serveCommand: 'swift run',
        entryFile: 'Package.swift',
        description: 'Swift Package',
      };
    }

    return {
      type: 'unknown',
      description: 'Unknown project type',
    };
  }

  // ──────────────────────────────────────────────────────────
  // Ecosystem-specific detection
  // ──────────────────────────────────────────────────────────

  private static detectNode(dir: string): ProjectType {
    const pkgPath = path.join(dir, 'package.json');
    let scripts: Record<string, string> = {};
    let name = 'Node.js project';

    try {
      const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
      scripts = pkg.scripts ?? {};
      if (pkg.name) name = pkg.name;
    } catch {
      // Malformed package.json — use defaults
    }

    const lockFile = fs.existsSync(path.join(dir, 'package-lock.json')) ? 'package-lock.json'
      : fs.existsSync(path.join(dir, 'yarn.lock')) ? 'yarn.lock'
      : fs.existsSync(path.join(dir, 'pnpm-lock.yaml')) ? 'pnpm-lock.yaml'
      : undefined;

    // Infer package manager from lock file
    const pm = lockFile === 'yarn.lock' ? 'yarn'
      : lockFile === 'pnpm-lock.yaml' ? 'pnpm'
      : 'npm';

    return {
      type: 'node',
      buildCommand: scripts.build ? `${pm} run build` : undefined,
      testCommand: scripts.test ? `${pm} test` : undefined,
      serveCommand: scripts.dev ? `${pm} run dev`
        : scripts.start ? `${pm} start`
        : undefined,
      lockFile,
      entryFile: 'package.json',
      description: `Node.js (${name})`,
    };
  }

  private static detectRust(dir: string): ProjectType {
    const cargoPath = path.join(dir, 'Cargo.toml');
    let name = 'Rust project';
    let hasBin = false;

    try {
      const cargo = fs.readFileSync(cargoPath, 'utf8');
      // Basic TOML parsing for name and [[bin]]
      const nameMatch = cargo.match(/^name\s*=\s*"([^"]+)"/m);
      if (nameMatch) name = nameMatch[1];
      hasBin = cargo.includes('[[bin]]') || fs.existsSync(path.join(dir, 'src', 'main.rs'));
    } catch {
      // Malformed Cargo.toml — use defaults
    }

    return {
      type: 'rust',
      buildCommand: 'cargo build',
      testCommand: 'cargo test',
      serveCommand: hasBin ? 'cargo run' : undefined,
      lockFile: fs.existsSync(path.join(dir, 'Cargo.lock')) ? 'Cargo.lock' : undefined,
      entryFile: 'Cargo.toml',
      description: `Rust (${name})`,
    };
  }

  private static detectPython(dir: string): ProjectType {
    const pyprojectPath = path.join(dir, 'pyproject.toml');
    let description = 'Python project';

    try {
      const content = fs.readFileSync(pyprojectPath, 'utf8');
      const nameMatch = content.match(/^name\s*=\s*"([^"]+)"/m);
      if (nameMatch) description = `Python (${nameMatch[1]})`;
    } catch {
      // ignore
    }

    // Detect build system
    const hasPoetry = fs.existsSync(path.join(dir, 'poetry.lock'));
    const hasUv = fs.existsSync(path.join(dir, 'uv.lock'));

    return {
      type: 'python',
      buildCommand: hasPoetry ? 'poetry build' : hasUv ? 'uv build' : 'python -m build',
      testCommand: hasPoetry ? 'poetry run pytest' : hasUv ? 'uv run pytest' : 'python -m pytest',
      serveCommand: hasPoetry ? 'poetry run python -m app' : undefined,
      lockFile: hasPoetry ? 'poetry.lock' : hasUv ? 'uv.lock' : undefined,
      entryFile: 'pyproject.toml',
      description,
    };
  }

  private static findXcodeProject(dir: string): string | undefined {
    try {
      const entries = fs.readdirSync(dir);
      // Prefer .xcworkspace over .xcodeproj
      const workspace = entries.find(e => e.endsWith('.xcworkspace'));
      if (workspace) return workspace;
      return entries.find(e => e.endsWith('.xcodeproj'));
    } catch {
      return undefined;
    }
  }
}
