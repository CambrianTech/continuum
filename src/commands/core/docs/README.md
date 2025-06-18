# Docs Command

## Definition
- **Name**: docs
- **Description**: Generate and manage documentation
- **Category**: Core  
- **Icon**: 📄
- **Status**: 🔴 BROKEN (Last working: 2025-06-10, Broken since: 2025-06-15)
- **Parameters**: `<action> [format] [output]`

## Overview  
⚠️ **COMMAND CURRENTLY BROKEN** - Do not use in production

The Docs command was intended to generate comprehensive documentation from various sources, but has multiple critical issues that prevent it from functioning.

## Parameters
- `action`: Documentation action (generate, update, validate)
- `format`: Output format (markdown, html, pdf)
- `output`: Output directory or file

## Usage Examples
```bash
# ❌ THESE WILL FAIL - Command is broken
python3 ai-portal.py --cmd docs --params '{"action": "generate"}'
python3 ai-portal.py --cmd docs --params '{"action": "update", "format": "markdown"}'
```

## ❌ CRITICAL ISSUES
1. **Missing dependencies**: pdf-generator package not installed
2. **Broken file paths**: Hardcoded paths that don't exist
3. **Memory leaks**: Large documentation causes server crashes
4. **Invalid output**: Generated HTML is malformed
5. **Race conditions**: Concurrent doc generation causes corruption

## Package Rules
```json
{
  "timeouts": {"client": 120.0, "server": 60.0},
  "retries": {"client": 0, "server": 0},
  "concurrency": {"client": false, "server": false},
  "sideEffects": ["file_system", "memory_intensive"],
  "status": "BROKEN"
}
```

## Status History
- 🔴 **2025-06-15**: BROKEN - Memory leak causes server crashes
- 🟡 **2025-06-12**: TESTING - PDF generation failing
- 🟢 **2025-06-10**: STABLE - Last known working version
- 🟡 **2025-06-08**: TESTING - Initial implementation

## TODO: Critical Fixes Required
- TODO: Fix memory leak in large document processing
- TODO: Replace hardcoded paths with workspace command
- TODO: Install missing pdf-generator dependencies  
- TODO: Fix HTML template rendering issues
- TODO: Add proper error handling for file operations
- TODO: Implement proper concurrent access controls

## Alternative Solutions
While this command is broken, you can use:
- `help --sync` for README generation
- Manual markdown files in docs/ directory
- External documentation tools

## DO NOT USE
⛔ This command will likely crash the server or produce corrupted output. Use alternatives until fixed.