# Dataset Commands

Commands for managing AI conversation history datasets for training.

## Commands

### `ai/dataset/create`
Create compressed archives of AI conversation history.

**Usage**:
```bash
./jtag ai/dataset/create --project=claude-continuum
./jtag ai/dataset/create --source=claude-projects
./jtag ai/dataset/create  # Archive all enabled projects
```

**Parameters**:
- `project` - Specific project ID to archive
- `source` - Filter by source ID
- `outputPath` - Override default output directory
- `compression` - Override compression type (gzip, bzip2, xz, none)
- `includeManifest` - Include manifest.json metadata (default: true)

### `ai/dataset/list`
List available dataset archives with metadata.

**Usage**:
```bash
./jtag ai/dataset/list
./jtag ai/dataset/list --detailed
./jtag ai/dataset/list --path=/custom/path
```

**Parameters**:
- `path` - Search path for archives (default: configured datasets directory)
- `detailed` - Show detailed information including manifests

## Configuration

### Custom Datasets Directory

Set the `DATASETS_DIR` environment variable to use a custom directory:

```bash
# In your shell profile (~/.zshrc, ~/.bashrc, etc.)
export DATASETS_DIR=/Volumes/FlashGordon/cambrian/datasets
```

Or add to `~/.continuum/config/environment`:
```bash
DATASETS_DIR=/Volumes/FlashGordon/cambrian/datasets
```

**Default**: If not set, archives are stored in `$HOME/.continuum/datasets`

### Dataset Configuration File

Create `~/.continuum/config/datasets.json` to customize sources and projects:

```json
{
  "version": "1.0.0",
  "defaultOutputPath": "/Volumes/FlashGordon/cambrian/datasets",
  "sources": [
    {
      "id": "claude-projects",
      "name": "Claude Projects",
      "type": "claude-projects",
      "basePath": "$HOME/.claude/projects",
      "enabled": true,
      "discoveryPattern": "-*"
    },
    {
      "id": "cursor-history",
      "name": "Cursor History",
      "type": "cursor-history",
      "basePath": "$HOME/.cursor/history",
      "enabled": false
    }
  ],
  "projects": [
    {
      "id": "claude-continuum",
      "name": "Continuum Project",
      "sourceId": "claude-projects",
      "path": "-Volumes-FlashGordon-cambrian-continuum",
      "enabled": true,
      "tags": ["continuum", "main"]
    }
  ],
  "compression": "gzip",
  "naming": "{project}-{timestamp}.tar.gz"
}
```

## Source Types

Supported source types:
- `claude-projects` - Claude Code project history
- `cursor-history` - Cursor IDE conversation history
- `vscode-chat` - VS Code chat history
- `continuum` - Continuum internal data
- `git` - Git repositories
- `custom` - Custom data sources

## Archive Format

Archives contain:
- Original project files/conversations
- `manifest.json` - Metadata (project info, size, file count, compression)

**Naming pattern** (customizable):
- `{project}` - Project ID
- `{timestamp}` - Full ISO timestamp (2025-01-07-145807)
- `{date}` - Date only (2025-01-07)
- `{time}` - Time only (145807)

## Future Commands

- `ai/dataset/import` - Transform archives + git repos into training format
- `ai/dataset/export` - Export to specific training formats (JSONL, etc.)
- `ai/dataset/merge` - Combine multiple archives
- `ai/dataset/verify` - Verify archive integrity
