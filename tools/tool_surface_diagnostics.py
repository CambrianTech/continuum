#!/usr/bin/env python3
"""Diagnose token cost of the tool surface area.

Parses Rust command registration files, extracts NAME/DESCRIPTION/Params schema info,
estimates JSON serialization sizes, and computes token costs using continuum_base's heuristic.
Produces a per-tool breakdown and totals.
"""

import json
import os
import re
from pathlib import Path
from dataclasses import dataclass, field

REPO_ROOT = Path(__file__).parent.parent  # repo root (tools/ is in it)


@dataclass
class CommandSpec:
    name: str
    description: str
    access_level: str
    param_fields: int = 0
    params_content: str = ""
    estimated_json_size: int = 0
    token_estimate: float = 0.0


def extract_command_specs(repo_root: Path) -> list[CommandSpec]:
    """Find all command registration files and extract their specs."""
    # Find all .rs files in the commands directories
    patterns = [
        "**/commands/**/*.rs",
        "**/runtime/registry.rs",
        "**/sdk_codegen/command.rs",
    ]

    found_files = []
    for pattern in patterns:
        found_files.extend(repo_root.glob(pattern))

    specs = []

    for filepath in found_files:
        if "test" in str(filepath).lower():
            continue
        if "target/" in str(filepath):
            continue

        content = filepath.read_text()

        # Extract NAME
        name_match = re.search(r'const\s+NAME:\s*&?\'static\s+str\s*=\s*"([^"]+)"', content)
        if not name_match:
            continue
        name = name_match.group(1)

        # Extract DESCRIPTION (handle multi-line strings)
        desc_match = re.search(r'const\s+DESCRIPTION:\s*&?\'static\s+str\s*=\s*"((?:[^"\\]|\\.)*)"', content, re.DOTALL)
        if not desc_match:
            continue
        description_raw = desc_match.group(1)
        # Clean escape sequences for realistic token estimation
        description_clean = description_raw.replace('\\n', ' ').replace('\\t', ' ').strip()

        # Extract Access Level
        access_match = re.search(r'const\s+ACCESS:\s*AccessLevel::(\w+)', content)
        if not access_match:
            continue
        access_level = access_match.group(1)

        # Skip non-AiSafe for now (we'll count them separately)
        if access_level != "AiSafe":
            continue

        # Extract Params struct fields
        params_content, field_count = extract_params_struct(content)

        spec = CommandSpec(
            name=name,
            description=description_clean,
            access_level=access_level,
            param_fields=field_count,
            params_content=params_content,
        )
        specs.append(spec)

    return specs


def extract_params_struct(content: str):
    """Extract the Params struct and count its fields."""
    # Find struct definition for Params
    params_match = re.search(r'pub\s+struct\s+(Params|[^{]+)\s*\{(.*?)\n\}', content, re.DOTALL)
    if not params_match:
        return "", 0

    struct_name = params_match.group(1).strip()
    struct_body = params_match.group(2)

    # Count fields (lines with pub or type annotations inside the struct)
    field_pattern = re.compile(r'(pub\s+)?(\w+)\s*:', re.MULTILINE)
    fields = field_pattern.findall(struct_body)

    return struct_body, len(fields)


def estimate_json_size(spec: CommandSpec) -> int:
    """Estimate JSON serialization size for a NativeToolSpec."""
    # Base structure: {"name": "...", "description": "...", "parameters": {...}}
    base_size = 45 + 20  # overhead

    # Name contribution
    name_json = json.dumps(spec.name)
    base_size += len(name_json)

    # Description contribution  
    desc_json = json.dumps(spec.description)
    base_size += len(desc_json)

    # Parameters schema estimation
    if spec.param_fields > 0:
        # Each field adds ~50-100 bytes for type info, required array, etc.
        per_field_estimate = 60 + (spec.param_fields * 30)
        base_size += per_field_estimate

        # Add content from params struct if available
        if spec.params_content:
            # Rough estimate: params JSON is ~2x the raw text due to quotes, braces, etc.
            schema_est = len(spec.params_content) * 2.5 + (spec.param_fields * 40)
            base_size += int(schema_est)

    return max(base_size, 150)  # Minimum reasonable size


def estimate_prompt_tokens(text: str) -> float:
    """continuum_base's heuristic: char/4+1 per token."""
    if not text:
        return 0.0
    return len(text) / 4.0 + 1.0


def compute_token_costs(specs: list[CommandSpec]) -> dict:
    """Compute token costs for the entire tool surface."""
    PER_TOOL_TEMPLATE_MARGIN = 8  # From continuum_base

    total_tokens = 0.0
    per_tool_data = []

    for spec in specs:
        json_size = estimate_json_size(spec)
        
        # Token estimation: description + schema + template overhead
        desc_tokens = estimate_prompt_tokens(spec.description)
        param_tokens = estimate_prompt_tokens(spec.params_content) if spec.params_content else 0.0
        
        total_tool_tokens = desc_tokens + param_tokens + PER_TOOL_TEMPLATE_MARGIN
        
        spec.token_estimate = total_tool_tokens
        spec.estimated_json_size = json_size
        
        per_tool_data.append({
            'name': spec.name,
            'description_len': len(spec.description),
            'param_fields': spec.param_fields,
            'json_size': json_size,
            'tokens': round(total_tool_tokens, 1)
        })
        
        total_tokens += total_tool_tokens

    return {
        'total_tools': len(specs),
        'total_json_bytes': sum(s.estimated_json_size for s in specs),
        'total_tokens': round(total_tokens, 1),
        'per_tool': sorted(per_tool_data, key=lambda x: x['tokens'], reverse=True)
    }


def print_report(results: dict):
    """Print a formatted diagnostic report."""
    print("=" * 80)
    print("TOOL SURFACE TOKEN DIAGNOSTIC REPORT")
    print("=" * 80)
    
    print(f"\nTotal AiSafe Tools Found: {results['total_tools']}")
    print(f"Total JSON Serialization Size: ~{results['total_json_bytes']:,} bytes")
    print(f"Total Token Cost (with template overhead): ~{results['total_tokens']:,.1f} tokens")
    
    print("\nTop 20 Largest Contributors:")
    print("-" * 80)
    for i, tool in enumerate(results['per_tool'][:20], 1):
        print(f"{i:2d}. {tool['name']:40s} | fields={tool['param_fields']:3d} | "
              f"json={tool['json_size']:6d}B | tokens={tool['tokens']:7.1f}")
    
    print("\nBottom 5 Contributors:")
    print("-" * 80)
    for tool in results['per_tool'][-5:]:
        print(f"{tool['name']:40s} | fields={tool['param_fields']:3d} | "
              f"json={tool['json_size']:6d}B | tokens={tool['tokens']:7.1f}")


def main():
    print("Scanning command registration files...")
    
    specs = extract_command_specs(REPO_ROOT)
    print(f"Found {len(specs)} AiSafe commands\n")
    
    if not specs:
        print("ERROR: No AiSafe commands found. Check file patterns.")
        return
    
    results = compute_token_costs(specs)
    print_report(results)


if __name__ == "__main__":
    main()
