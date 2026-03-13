/**
 * TypesFilePatcher — Domain-agnostic TypeScript file patcher
 *
 * Applies surgical code injections to existing TypeScript files.
 * Designed for reuse across commands, entities, widgets, and Rust IPC workers.
 *
 * Operations:
 *   - addImport:   Add an import statement if not already present
 *   - injectAfter: Insert code after a regex match
 *   - append:      Append code at end of file
 *   - replace:     Replace a matched section
 *
 * All operations are idempotent — applying a patch twice produces the same result.
 */

import * as fs from 'fs';

// ── Patch Operations ────────────────────────────────────────────────────

export type PatchOperationType = 'addImport' | 'injectAfter' | 'append' | 'replace';

export interface AddImportOp {
  type: 'addImport';
  /** The import statement to add (without trailing semicolon is fine — we normalize) */
  importLine: string;
  /** String to check for idempotency — if present, skip this import */
  guard: string;
}

export interface InjectAfterOp {
  type: 'injectAfter';
  /** Regex pattern to find the insertion point */
  pattern: RegExp;
  /** Code to inject after the match */
  code: string;
  /** String to check for idempotency */
  guard: string;
}

export interface AppendOp {
  type: 'append';
  /** Code to append at end of file */
  code: string;
  /** String to check for idempotency */
  guard: string;
}

export interface ReplaceOp {
  type: 'replace';
  /** Regex pattern to match the section to replace */
  pattern: RegExp;
  /** Replacement code */
  code: string;
}

export type PatchOperation = AddImportOp | InjectAfterOp | AppendOp | ReplaceOp;

// ── Patch Result ────────────────────────────────────────────────────────

export interface PatchResult {
  /** Whether the file was modified */
  modified: boolean;
  /** Operations that were applied */
  applied: string[];
  /** Operations that were skipped (idempotency guard matched) */
  skipped: string[];
  /** Operations that failed (pattern not found, etc.) */
  errors: string[];
}

// ── TypesFilePatcher ────────────────────────────────────────────────────

export class TypesFilePatcher {

  /**
   * Apply a set of patch operations to a file's content.
   * Returns the modified content and a result summary.
   */
  static patch(content: string, operations: PatchOperation[]): { content: string; result: PatchResult } {
    const result: PatchResult = {
      modified: false,
      applied: [],
      skipped: [],
      errors: [],
    };

    let current = content;

    for (const op of operations) {
      switch (op.type) {
        case 'addImport':
          current = this.applyAddImport(current, op, result);
          break;
        case 'injectAfter':
          current = this.applyInjectAfter(current, op, result);
          break;
        case 'append':
          current = this.applyAppend(current, op, result);
          break;
        case 'replace':
          current = this.applyReplace(current, op, result);
          break;
      }
    }

    result.modified = current !== content;
    return { content: current, result };
  }

  /**
   * Apply patches to a file on disk. Reads, patches, writes.
   */
  static patchFile(filePath: string, operations: PatchOperation[]): PatchResult {
    const content = fs.readFileSync(filePath, 'utf-8');
    const { content: patched, result } = this.patch(content, operations);

    if (result.modified) {
      fs.writeFileSync(filePath, patched, 'utf-8');
    }

    return result;
  }

  // ── Private Applicators ─────────────────────────────────────────────

  private static applyAddImport(content: string, op: AddImportOp, result: PatchResult): string {
    if (content.includes(op.guard)) {
      result.skipped.push(`addImport: ${op.guard} (already present)`);
      return content;
    }

    // Find the last import line and insert after it
    const lines = content.split('\n');
    let lastImportIdx = -1;

    for (let i = 0; i < lines.length; i++) {
      if (/^import\s/.test(lines[i])) {
        lastImportIdx = i;
      }
    }

    const importLine = op.importLine.endsWith(';') ? op.importLine : op.importLine + ';';

    if (lastImportIdx >= 0) {
      lines.splice(lastImportIdx + 1, 0, importLine);
    } else {
      // No imports found — add at top (after any leading comments/shebang)
      let insertIdx = 0;
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i].trim();
        if (line.startsWith('#!') || line.startsWith('/**') || line.startsWith(' *') || line.startsWith('*/') || line === '') {
          insertIdx = i + 1;
        } else {
          break;
        }
      }
      lines.splice(insertIdx, 0, importLine);
    }

    result.applied.push(`addImport: ${op.guard}`);
    return lines.join('\n');
  }

  private static applyInjectAfter(content: string, op: InjectAfterOp, result: PatchResult): string {
    if (content.includes(op.guard)) {
      result.skipped.push(`injectAfter: ${op.guard} (already present)`);
      return content;
    }

    const match = op.pattern.exec(content);
    if (!match) {
      result.errors.push(`injectAfter: pattern not found — ${op.pattern.source}`);
      return content;
    }

    const insertPos = match.index + match[0].length;
    const before = content.slice(0, insertPos);
    const after = content.slice(insertPos);

    result.applied.push(`injectAfter: ${op.guard}`);
    return before + '\n' + op.code + after;
  }

  private static applyAppend(content: string, op: AppendOp, result: PatchResult): string {
    if (content.includes(op.guard)) {
      result.skipped.push(`append: ${op.guard} (already present)`);
      return content;
    }

    result.applied.push(`append: ${op.guard}`);
    return content.trimEnd() + '\n' + op.code + '\n';
  }

  private static applyReplace(content: string, op: ReplaceOp, result: PatchResult): string {
    const match = op.pattern.exec(content);
    if (!match) {
      result.errors.push(`replace: pattern not found — ${op.pattern.source}`);
      return content;
    }

    result.applied.push(`replace: ${op.pattern.source}`);
    return content.slice(0, match.index) + op.code + content.slice(match.index + match[0].length);
  }
}
