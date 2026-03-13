/**
 * CommandFixerStrategies — Command-specific patch builders
 *
 * Analyzes a command's Types file and generates PatchOperation[] to inject
 * missing static accessors and factory functions. Reads the actual interface
 * definitions to produce correctly typed output.
 *
 * Designed to be called from CommandGeneratorType.fixOne() — replaces the
 * "add manually" punt with real surgical injection.
 */

import type { PatchOperation } from './TypesFilePatcher';

// ── Extracted Type Info ─────────────────────────────────────────────────

export interface ExtractedTypeInfo {
  /** Params interface name (e.g., 'AgentListParams') */
  paramsName: string;
  /** Result interface name (e.g., 'AgentListResult') */
  resultName: string;
  /** Whether result has generic parameters */
  resultHasGeneric: boolean;
  /** Generic constraint without default (e.g., 'T extends BaseEntity') */
  resultGenericParam?: string;
  /** Generic type name if present (e.g., 'T') */
  resultGenericName?: string;
  /** Default type for generic (e.g., 'BaseEntity') */
  resultGenericDefault?: string;
  /** Fields from Params interface (for factory defaults) */
  paramFields: InterfaceField[];
  /** Fields from Result interface (for factory defaults) */
  resultFields: InterfaceField[];
  /** PascalCase class name (e.g., 'AgentList') */
  className: string;
  /** Command path (e.g., 'agent/list') */
  commandPath: string;
}

export interface InterfaceField {
  name: string;
  type: string;
  optional: boolean;
  comment?: string;
}

// ── Extraction ──────────────────────────────────────────────────────────

/**
 * Extract Params and Result interface info from a Types file's content.
 * Returns null if the file doesn't have recognizable Params/Result interfaces.
 */
export function extractTypeInfo(content: string, commandName: string): ExtractedTypeInfo | null {
  // Extract Params interface name
  const paramsMatch = content.match(/export\s+interface\s+(\w+Params)\s+extends\s+CommandParams/);
  if (!paramsMatch) return null;
  const paramsName = paramsMatch[1];

  // Extract Result interface name
  const resultMatch = content.match(
    /export\s+interface\s+(\w+Result)(?:<([^>]+)>)?\s+extends\s+(?:CommandResult|JTAGPayload)/
  );
  if (!resultMatch) return null;
  const resultName = resultMatch[1];
  const resultGenericParam = resultMatch[2] || undefined;

  // Parse generic info
  // Generic can be: "T", "T extends Foo", or "T extends Foo = Foo"
  let resultHasGeneric = false;
  let resultGenericName: string | undefined;
  let resultGenericDefault: string | undefined;
  // Normalize: resultGenericParam stores constraint only (no default)
  let normalizedGenericParam: string | undefined;
  if (resultGenericParam) {
    resultHasGeneric = true;
    // Check if a default is already specified (e.g., "T extends BaseEntity = BaseEntity")
    const defaultSplit = resultGenericParam.split('=');
    if (defaultSplit.length > 1) {
      const constraint = defaultSplit[0].trim(); // "T extends BaseEntity"
      resultGenericDefault = defaultSplit[1].trim(); // "BaseEntity"
      const constraintParts = constraint.split(/\s+extends\s+/);
      resultGenericName = constraintParts[0].trim(); // "T"
      normalizedGenericParam = constraint;
    } else {
      const parts = resultGenericParam.split(/\s+extends\s+/);
      resultGenericName = parts[0].trim();
      resultGenericDefault = parts.length > 1 ? parts[1].trim() : 'unknown';
      normalizedGenericParam = resultGenericParam;
    }
  }

  // Extract fields from Params interface
  const paramFields = extractInterfaceFields(content, paramsName);
  // Extract fields from Result interface
  const resultFields = extractInterfaceFields(content, resultName);

  // Build class name from command name: "agent/list" → "AgentList"
  const className = commandName
    .split(/[\/\-_]/)
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');

  return {
    paramsName,
    resultName,
    resultHasGeneric,
    resultGenericParam: normalizedGenericParam,
    resultGenericName,
    resultGenericDefault,
    paramFields,
    resultFields,
    className,
    commandPath: commandName,
  };
}

/**
 * Extract fields from a TypeScript interface body.
 * Skips inherited fields (context, sessionId, userId, success, error, _noParams).
 */
function extractInterfaceFields(content: string, interfaceName: string): InterfaceField[] {
  const fields: InterfaceField[] = [];

  // Find the interface block — handle generics in the name
  const escapedName = interfaceName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const regex = new RegExp(
    `export\\s+interface\\s+${escapedName}(?:<[^>]*>)?\\s+extends\\s+\\w+[^{]*\\{([\\s\\S]*?)\\n\\}`,
    'm'
  );
  const match = content.match(regex);
  if (!match) return fields;

  const body = match[1];
  const inherited = new Set(['context', 'sessionId', 'userId', 'success', 'error', '_noParams']);
  const seen = new Set<string>();

  // Line-by-line field extraction — simpler and more reliable than complex regex
  const lines = body.split('\n');
  let lastComment: string | undefined;

  for (const line of lines) {
    const trimmed = line.trim();

    // Track comments
    if (trimmed.startsWith('//') || trimmed.startsWith('/**') || trimmed.startsWith('*')) {
      const commentText = trimmed.replace(/^\/\/\s*|^\/\*\*?\s*|\*\/\s*$|^\*\s*/g, '').trim();
      if (commentText) lastComment = commentText;
      continue;
    }

    // Skip empty lines (but preserve last comment)
    if (!trimmed) continue;

    // Match field: name?: type;
    const fieldMatch = trimmed.match(/^(\w+)(\?)?\s*:\s*(.+?)\s*;?\s*$/);
    if (!fieldMatch) {
      lastComment = undefined;
      continue;
    }

    const [, name, optional, type] = fieldMatch;
    if (inherited.has(name) || seen.has(name)) {
      lastComment = undefined;
      continue;
    }

    seen.add(name);
    fields.push({
      name,
      type: type.replace(/;$/, '').trim(),
      optional: !!optional,
      ...(lastComment ? { comment: lastComment } : {}),
    });
    lastComment = undefined;
  }

  return fields;
}

// ── Patch Generation ────────────────────────────────────────────────────

/**
 * Generate PatchOperations to add missing factory functions to a Types file.
 */
export function generateFactoryPatches(info: ExtractedTypeInfo, content: string): PatchOperation[] {
  const ops: PatchOperation[] = [];

  // ── Required imports ──────────────────────────────────────────────
  if (!content.includes('createPayload')) {
    ops.push({
      type: 'addImport',
      importLine: "import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';",
      guard: 'createPayload',
    });
  } else if (!content.includes('transformPayload')) {
    // createPayload exists but transformPayload might be missing
    // Need to add transformPayload to existing import — complex, just check
    if (!content.includes('transformPayload')) {
      ops.push({
        type: 'addImport',
        importLine: "import { transformPayload } from '@system/core/types/JTAGTypes';",
        guard: 'transformPayload',
      });
    }
  }

  // JTAGContext must be a proper top-level import, not just an inline import() type
  const hasJTAGContextImport = /import\s+(?:type\s+)?{[^}]*JTAGContext[^}]*}\s+from/.test(content);
  if (!hasJTAGContextImport) {
    ops.push({
      type: 'addImport',
      importLine: "import type { JTAGContext } from '@system/core/types/JTAGTypes';",
      guard: 'import type { JTAGContext }',
    });
  }

  if (!/import\s+{[^}]*SYSTEM_SCOPES[^}]*}\s+from/.test(content)) {
    ops.push({
      type: 'addImport',
      importLine: "import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';",
      guard: 'SYSTEM_SCOPES',
    });
  }

  if (!/import\s+(?:type\s+)?{[^}]*\bUUID\b[^}]*}\s+from/.test(content)) {
    ops.push({
      type: 'addImport',
      importLine: "import type { UUID } from '@system/core/types/CrossPlatformUUID';",
      guard: "CrossPlatformUUID",
    });
  }

  // ── createParams factory ──────────────────────────────────────────
  // Use Omit<ParamsType, inherited> as the data type — avoids reconstructing fields
  const createParamsFnName = `create${info.className}Params`;
  if (!content.includes(createParamsFnName)) {
    ops.push({
      type: 'append',
      guard: createParamsFnName,
      code: `
/**
 * Factory function for creating ${info.className}Params
 */
export const ${createParamsFnName} = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<${info.paramsName}, 'context' | 'sessionId' | 'userId'>
): ${info.paramsName} => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});
`,
    });
  }

  // ── createResult factory ──────────────────────────────────────────
  const createResultFnName = `create${info.className}Result`;
  if (!content.includes(createResultFnName)) {
    let resultType = info.resultName;
    let genericPrefix = '';
    if (info.resultHasGeneric) {
      resultType = `${info.resultName}<${info.resultGenericName}>`;
      genericPrefix = `<${info.resultGenericParam} = ${info.resultGenericDefault}>`;
    }

    ops.push({
      type: 'append',
      guard: createResultFnName,
      code: `
/**
 * Factory function for creating ${info.className}Result with defaults
 */
export const ${createResultFnName} = ${genericPrefix}(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<${resultType}, 'context' | 'sessionId' | 'userId'>
): ${resultType} => createPayload(context, sessionId, {
  ...data
});
`,
    });
  }

  // ── createResultFromParams ────────────────────────────────────────
  const createFromParamsFnName = `create${info.className}ResultFromParams`;
  if (!content.includes(createFromParamsFnName)) {
    let resultType = info.resultName;
    if (info.resultHasGeneric) {
      resultType = `${info.resultName}<${info.resultGenericName}>`;
    }
    let genericPrefix = '';
    if (info.resultHasGeneric) {
      genericPrefix = `<${info.resultGenericParam} = ${info.resultGenericDefault}>`;
    }

    ops.push({
      type: 'append',
      guard: createFromParamsFnName,
      code: `
/**
 * Smart ${info.commandPath}-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const ${createFromParamsFnName} = ${genericPrefix}(
  params: ${info.paramsName},
  differences: Omit<${resultType}, 'context' | 'sessionId' | 'userId'>
): ${resultType} => transformPayload(params, differences);
`,
    });
  }

  return ops;
}

/**
 * Generate PatchOperations to add a missing static accessor to a Types file.
 */
export function generateAccessorPatches(info: ExtractedTypeInfo, content: string): PatchOperation[] {
  const ops: PatchOperation[] = [];

  // Check if accessor already exists
  if (content.includes(`export const ${info.className} =`)) {
    return ops;
  }

  // Ensure Commands import
  if (!content.includes('Commands')) {
    ops.push({
      type: 'addImport',
      importLine: "import { Commands } from '@system/core/shared/Commands';",
      guard: 'Commands',
    });
  }

  // Ensure CommandInput import
  if (!content.includes('CommandInput')) {
    ops.push({
      type: 'addImport',
      importLine: "import type { CommandInput } from '@system/core/types/JTAGTypes';",
      guard: 'CommandInput',
    });
  }

  // Build accessor
  let executeSignature: string;
  let executeBody: string;

  if (info.resultHasGeneric) {
    const genericParam = info.resultGenericParam;
    const genericName = info.resultGenericName;
    const defaultType = info.resultGenericDefault;
    executeSignature = `execute<${genericParam} = ${defaultType}>(params: CommandInput<${info.paramsName}>): Promise<${info.resultName}<${genericName}>>`;
    executeBody = `Commands.execute<${info.paramsName}, ${info.resultName}<${genericName}>>('${info.commandPath}', params as Partial<${info.paramsName}>)`;
  } else {
    executeSignature = `execute(params: CommandInput<${info.paramsName}>): Promise<${info.resultName}>`;
    executeBody = `Commands.execute<${info.paramsName}, ${info.resultName}>('${info.commandPath}', params as Partial<${info.paramsName}>)`;
  }

  ops.push({
    type: 'append',
    guard: `export const ${info.className} =`,
    code: `
/**
 * ${info.className} — Type-safe command executor
 *
 * Usage:
 *   import { ${info.className} } from '...shared/${info.className}Types';
 *   const result = await ${info.className}.execute({ ... });
 */
export const ${info.className} = {
  ${executeSignature} {
    return ${executeBody};
  },
  commandName: '${info.commandPath}' as const,
} as const;
`,
  });

  return ops;
}

// ── Helpers ─────────────────────────────────────────────────────────────

function getDefaultForType(type: string): string {
  const trimmed = type.trim();
  if (trimmed === 'string') return "''";
  if (trimmed === 'number') return '0';
  if (trimmed === 'boolean') return 'false';
  if (trimmed.endsWith('[]') || trimmed.startsWith('Array<')) return '[]';
  if (trimmed === 'object' || trimmed.startsWith('{')) return '{}';
  if (trimmed.startsWith('Record<')) return '{}';
  // For union types, optional types, etc. — use empty string as safe default
  return "'' as never";
}
