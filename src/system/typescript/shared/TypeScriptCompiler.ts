/**
 * TypeScript Compiler Foundation
 *
 * Provides clean API for TypeScript compiler operations:
 * - Type reflection (extract interfaces, parameters, inheritance)
 * - Compilation (with proper module resolution)
 * - Validation (runtime type checking)
 * - Analysis (static analysis, linting)
 * - Transformation (AST modifications)
 *
 * Used by commands: code/reflect, code/compile, code/validate, schema/generate, etc.
 */

import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

export interface TypeScriptCompilerOptions {
  /** Root directory for the project */
  rootDir: string;

  /** TypeScript compiler options (defaults to tsconfig.json if not provided) */
  compilerOptions?: ts.CompilerOptions;

  /** Files to include in the program */
  files?: string[];
}

export interface InterfaceInfo {
  name: string;
  filePath: string;
  properties: PropertyInfo[];
  extends: string[];
}

export interface PropertyInfo {
  name: string;
  type: string;
  required: boolean;
  description?: string;
}

/**
 * TypeScript Compiler wrapper with proper module resolution
 */
export class TypeScriptCompiler {
  private program: ts.Program;
  private typeChecker: ts.TypeChecker;
  private rootDir: string;

  constructor(options: TypeScriptCompilerOptions) {
    this.rootDir = options.rootDir;

    // Load tsconfig.json if no compiler options provided
    const compilerOptions = options.compilerOptions || this.loadTsConfig();

    // Create program with all files (enables proper module resolution)
    const files = options.files || this.findAllTypeScriptFiles();

    this.program = ts.createProgram(files, compilerOptions);
    this.typeChecker = this.program.getTypeChecker();

    console.log(`🔧 TypeScriptCompiler initialized with ${files.length} files`);
  }

  /**
   * Load tsconfig.json from root directory
   */
  private loadTsConfig(): ts.CompilerOptions {
    const tsconfigPath = path.join(this.rootDir, 'tsconfig.json');

    if (fs.existsSync(tsconfigPath)) {
      const configFile = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
      const parsedConfig = ts.parseJsonConfigFileContent(
        configFile.config,
        ts.sys,
        this.rootDir
      );
      return parsedConfig.options;
    }

    // Default compiler options if no tsconfig.json
    return {
      target: ts.ScriptTarget.Latest,
      module: ts.ModuleKind.CommonJS,
      moduleResolution: ts.ModuleResolutionKind.NodeJs,
      esModuleInterop: true,
      skipLibCheck: true
    };
  }

  /**
   * Find all TypeScript files in root directory
   */
  private findAllTypeScriptFiles(): string[] {
    const files: string[] = [];

    const scan = (dir: string) => {
      if (!fs.existsSync(dir)) return;

      const entries = fs.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);

        // Skip node_modules, dist, etc.
        if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name.startsWith('.')) {
          continue;
        }

        if (entry.isDirectory()) {
          scan(fullPath);
        } else if (entry.name.endsWith('.ts') && !entry.name.endsWith('.d.ts')) {
          files.push(fullPath);
        }
      }
    };

    scan(this.rootDir);
    return files;
  }

  /**
   * Get all properties of an interface INCLUDING inherited properties
   * This is the key method for resolving cross-file inheritance
   */
  getInterfaceInfo(interfaceName: string, filePath: string): InterfaceInfo | null {
    const sourceFile = this.program.getSourceFile(filePath);
    if (!sourceFile) {
      console.warn(`⚠️  Source file not found: ${filePath}`);
      return null;
    }

    // Find the interface declaration
    let interfaceDecl: ts.InterfaceDeclaration | null = null;
    ts.forEachChild(sourceFile, node => {
      if (ts.isInterfaceDeclaration(node) && node.name.text === interfaceName) {
        interfaceDecl = node;
      }
    });

    if (!interfaceDecl) {
      console.warn(`⚠️  Interface '${interfaceName}' not found in ${filePath}`);
      return null;
    }

    // Use TypeChecker to get ALL properties including inherited ones
    const type = this.typeChecker.getTypeAtLocation(interfaceDecl);
    const properties = this.typeChecker.getPropertiesOfType(type);

    // Extract property information
    const propertyInfos: PropertyInfo[] = [];
    for (const prop of properties) {
      const propName = prop.getName();
      const propType = this.typeChecker.getTypeOfSymbolAtLocation(prop, interfaceDecl);
      const isOptional = !!(prop.flags & ts.SymbolFlags.Optional);
      const typeString = this.typeChecker.typeToString(propType);

      // Extract JSDoc description
      const jsDocTags = (prop as any).getJsDocTags?.() || [];
      const description = jsDocTags.length > 0 ? jsDocTags[0].text : undefined;

      propertyInfos.push({
        name: propName,
        type: typeString,
        required: !isOptional,
        description
      });
    }

    // Get extended interfaces (TypeScript narrowing workaround)
    const decl: ts.InterfaceDeclaration = interfaceDecl;
    const extendsNames: string[] = [];
    if (decl.heritageClauses) {
      for (const heritage of decl.heritageClauses) {
        for (const type of heritage.types) {
          extendsNames.push(type.expression.getText());
        }
      }
    }

    return {
      name: interfaceName,
      filePath,
      properties: propertyInfos,
      extends: extendsNames
    };
  }

  /**
   * Find all interfaces matching a pattern (e.g., "*Params")
   */
  findInterfaces(pattern: RegExp, searchDir?: string): Array<{ name: string; filePath: string }> {
    const results: Array<{ name: string; filePath: string }> = [];
    const dir = searchDir || this.rootDir;

    for (const sourceFile of this.program.getSourceFiles()) {
      // Skip node_modules, lib files
      if (sourceFile.fileName.includes('node_modules') || sourceFile.fileName.includes('lib.')) {
        continue;
      }

      // Check if file is in search directory
      if (searchDir && !sourceFile.fileName.startsWith(searchDir)) {
        continue;
      }

      ts.forEachChild(sourceFile, node => {
        if (ts.isInterfaceDeclaration(node)) {
          const name = node.name.text;
          if (pattern.test(name)) {
            results.push({
              name,
              filePath: sourceFile.fileName
            });
          }
        }
      });
    }

    return results;
  }

  /**
   * Compile TypeScript files
   */
  compile(): ts.Diagnostic[] {
    const emitResult = this.program.emit();
    const diagnostics = ts.getPreEmitDiagnostics(this.program).concat(emitResult.diagnostics);
    return diagnostics;
  }

  /**
   * Get the TypeChecker for advanced operations
   */
  getTypeChecker(): ts.TypeChecker {
    return this.typeChecker;
  }

  /**
   * Get the Program for advanced operations
   */
  getProgram(): ts.Program {
    return this.program;
  }
}
