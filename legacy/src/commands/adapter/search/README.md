# Adapter Search Command

Search for LoRA adapters across registries (HuggingFace, local, mesh)

## Table of Contents

- [Usage](#usage)
  - [CLI Usage](#cli-usage)
  - [Tool Usage](#tool-usage)
- [Parameters](#parameters)
- [Result](#result)
- [Examples](#examples)
- [Testing](#testing)
- [Access Level](#access-level)

## Usage

### CLI Usage

```bash
# Basic search
./jtag adapter/search --query="code generation"

# Search with filters
./jtag adapter/search --query="tool calling" --baseModel="llama" --limit=5

# Search only local adapters
./jtag adapter/search --query="my-adapter" --source="local"

# Search HuggingFace sorted by likes
./jtag adapter/search --query="sql" --source="huggingface" --sort="likes"
```

### Tool Usage

From Persona tools or programmatic access:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('adapter/search', {
  query: 'tool calling',
  baseModel: 'llama',
  limit: 5,
});

// Result contains array of adapters
for (const adapter of result.results) {
  console.log(`${adapter.name} - ${adapter.downloads} downloads`);
}
```

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `query` | string | Yes | - | Search query (adapter name, description, or capability) |
| `baseModel` | string | No | - | Filter by base model (e.g., 'llama', 'qwen', 'mistral') |
| `limit` | number | No | 10 | Maximum results to return |
| `source` | string | No | 'all' | Source to search: 'all', 'huggingface', 'local', 'mesh' |
| `sort` | string | No | 'downloads' | Sort by: 'downloads', 'likes', 'recent' |

## Result

Returns `AdapterSearchResult`:

```typescript
{
  success: true,
  results: AdapterSearchResultItem[],  // Matching adapters
  totalCount: number,                   // Total matches (may exceed limit)
  query: string,                        // Query executed
  searchTimeMs: number,                 // Search duration
  sourcesSearched: string[],            // Which sources were queried
}
```

### AdapterSearchResultItem

Each result item contains:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique identifier (HF repo ID or local path) |
| `name` | string | Display name |
| `description` | string | Description or model card excerpt |
| `baseModel` | string | Target base model (e.g., "meta-llama/Llama-3.2-3B-Instruct") |
| `source` | string | Source registry ('huggingface', 'local', 'mesh') |
| `downloads` | number | Download count (HuggingFace) |
| `likes` | number | Likes count (HuggingFace) |
| `lastModified` | string | ISO date of last modification |
| `author` | string | Author/organization |
| `tags` | string[] | Tags (task type, language, etc.) |
| `rank` | number | LoRA rank (if known) |
| `installed` | boolean | Whether adapter is installed locally |
| `localPath` | string | Local path if installed |

## Examples

### Search for Tool-Calling Adapters

```bash
./jtag adapter/search --query="tool calling" --baseModel="llama" --limit=5
```

**Result:**
```json
{
  "success": true,
  "results": [
    {
      "id": "codelion/Llama-3.2-1B-Instruct-tool-calling-lora",
      "name": "codelion/Llama-3.2-1B-Instruct-tool-calling-lora",
      "baseModel": "meta-llama/Llama-3.2-1B-Instruct",
      "downloads": 50,
      "likes": 4,
      "installed": false
    }
  ],
  "totalCount": 3,
  "searchTimeMs": 98
}
```

### Search Local Adapters Only

```bash
./jtag adapter/search --query="llama" --source="local"
```

### Search for SQL Adapters by Popularity

```bash
./jtag adapter/search --query="text-to-sql" --sort="downloads" --limit=10
```

## Integration with Adapter Commands

Search results can be used with other adapter commands:

```bash
# 1. Search for an adapter
./jtag adapter/search --query="code generation" --baseModel="llama"

# 2. Download it (using adapter ID from search)
# ./jtag adapter/download --repoId="codelion/Llama-3.2-1B-Instruct-tool-calling-lora"

# 3. Apply to genome
# ./jtag adapter/apply --adapterId="my-adapter" --scale=0.8
```

## Source Registries

| Source | Description |
|--------|-------------|
| `huggingface` | HuggingFace Hub - largest public LoRA repository |
| `local` | Local registry at `~/.continuum/adapters/installed/` |
| `mesh` | Federated mesh network (coming soon) |

## Access Level

**ai-safe** - Safe for AI personas to call autonomously. Personas can search for adapters that match their current task needs.

## Implementation Notes

- HuggingFace API is called with `filter=peft` to find LoRA/PEFT adapters
- Base model is extracted from both `cardData.base_model` and tags
- Local adapters store metadata in `manifest.json`
- Search is case-insensitive
- Results are deduplicated across sources
- Installed adapters are marked with `installed: true`
