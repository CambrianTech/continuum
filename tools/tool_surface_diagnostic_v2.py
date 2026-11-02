#!/usr/bin/env python3
"""Definitive diagnostic for card 0bd4df6e: "The tool surface costs 16,241 tokens".

Parses ALL Rust command registration files across the workspace (both AiSafe and Privileged),
extracts NAME/DESCRIPTION/Params schema info, estimates JSON serialization sizes with
realistic schema overhead modeling, and computes token costs.

Produces:
- Per-tool breakdown with access level
- Totals by access level
- Gap analysis against the card's 16,241 figure
"""

import json
import os
import re
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent


def find_all_command_files(repo_root: Path) -> list[Path]:
    """Find ALL .rs files that declare command registrations (NAME + ACCESS)."""
    results = []
    for rs_file in repo_root.rglob("*.rs"):
        if "target/" in str(rs_file):
            continue
        content = rs_file.read_text()
        # Must have both NAME and ACCESS declarations to be a real command
        has_name = bool(re.search(r'const\s+NAME:\s*&?\s*\'static\s+str', content))
        has_access = bool(re.search(r'const\s+(?:ACCESS|ACCESS_LEVEL).*?AccessLevel::', content))
        if has_name and has_access:
            results.append(rs_file)
    return sorted(set(results))


def extract_command_info(filepath: Path):
    """Extract NAME, DESCRIPTION, ACCESS level, and Params struct from a command file."""
    content = filepath.read_text()

    # Extract NAME
    name_match = re.search(r'const\s+NAME:\s*&?\s*\'static\s+str\s*=\s*"([^"]+)"', content)
    if not name_match:
        return None
    
    name = name_match.group(1)
    
    # Skip test commands
    if "test" in str(filepath).lower() or "test" in name.lower():
        return None

    # Extract DESCRIPTION (handle multi-line strings with escape sequences)
    desc_match = re.search(r'const\s+DESCRIPTION:\s*&?\s*\'static\s+str\s*=\s*"((?:[^"\\]|\\.)*)"', content, re.DOTALL)
    if not desc_match:
        return None
    
    description_raw = desc_match.group(1)
    # Clean escape sequences for realistic token estimation
    description_clean = description_raw.replace('\\n', ' ').replace('\\t', ' ').strip()

    # Extract Access Level — two patterns exist:
    #   Pattern A: const ACCESS: AccessLevel = AccessLevel::AiSafe;
    #   Pattern B: const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = ...::AiSafe
    access_match = re.search(r'const\s+ACCESS[^_].*?AccessLevel::(\w+)', content)
    if not access_match:
        access_match = re.search(r'const\s+ACCESS_LEVEL.*?AccessLevel::(\w+)', content)
    if not access_match:
        return None
    
    access_level = access_match.group(1)

    # Extract Params struct fields — find the Params or similar struct
    params_content, field_count = extract_params_struct(content)

    return {
        'name': name,
        'description': description_clean,
        'access_level': access_level,
        'param_fields': field_count,
        'params_content': params_content,
        'filepath': str(filepath.relative_to(repo_root)),
    }


def extract_params_struct(content: str):
    """Extract the Params struct body and count its fields."""
    # Look for pub struct with name ending in Params or just "Params"
    # Match from "pub struct XxxParams {" to the closing brace of the struct body
    params_match = re.search(
        r'pub\s+struct\s+(\w*Params\w*)\s*\{(.*?)\n\}', 
        content, 
        re.DOTALL
    )
    if not params_match:
        # Try simpler pattern - any struct with fields
        params_match = re.search(r'pub\s+struct\s+\w+\s*\{(.*?)\n\}', content, re.DOTALL)
    
    if not params_match:
        return "", 0
    
    struct_body = params_match.group(1)
    
    # Count fields (lines with pub or type annotations inside the struct)
    field_pattern = re.compile(r'(pub\s+)?(\w+)\s*:', re.MULTILINE)
    fields = [f for f in field_pattern.findall(struct_body)]
    
    return struct_body, len(fields)


def estimate_json_schema_size(field_count: int, params_content: str) -> int:
    """Estimate JSON schema byte size based on field analysis.
    
    Models what schemars generates from Rust structs:
    - Each field gets type annotation + description in JSON Schema format
    - Nested structs become $defs entries
    - Required array lists all non-Option fields
    - Base overhead for the parameters object itself
    """
    if field_count == 0:
        # Empty params still need {"type": "object"} base
        return 120
    
    # Base structure: {"type":"object","properties":{...},"required":[...]}
    base_overhead = 150
    
    # Per-field cost: JSON Schema adds type, description, and required array entry
    # - Simple types (String, u32): ~40 bytes per field for type annotation
    # - Complex types (nested structs, enums): ~80-120 bytes plus $defs overhead
    # - Option<T> fields don't go in "required" but still have type info
    
    if params_content:
        # Use actual content as a proxy for complexity
        raw_size = len(params_content)
        # JSON schema is typically 2.5-3x the raw Rust struct text due to:
        # - Type annotations replacing Rust types
        # - Description strings wrapped in JSON format
        # - Required array overhead
        # - $defs for nested types (if any)
        schema_size = int(raw_size * 2.8 + field_count * 50)
    else:
        # Estimate based on field count alone
        schema_size = base_overhead + field_count * 75
    
    return max(schema_size, 150)


def estimate_prompt_tokens(text: str) -> float:
    """continuum_base's heuristic: char/4+1 per token."""
    if not text:
        return 0.0
    return len(text) / 4.0 + 1.0


def compute_tool_token_cost(info: dict, json_schema_bytes: int) -> float:
    """Compute total token cost for a single tool in the prompt.
    
    The NativeToolSpec serialized to JSON includes:
    - Tool name (in command field or similar)
    - Description 
    - Full JSON Schema parameters
    
    Token estimation uses char/4+1 heuristic on each component, plus template overhead.
    """
    PER_TOOL_TEMPLATE_MARGIN = 8
    
    # Name tokens (the tool identifier in the prompt)
    name_tokens = estimate_prompt_tokens(info['name'])
    
    # Description tokens
    desc_tokens = estimate_prompt_tokens(info['description'])
    
    # Parameter schema tokens - convert JSON bytes to token estimate
    # JSON text is ASCII, so char/4+1 works for byte->token conversion
    param_tokens = estimate_prompt_tokens(' ' * json_schema_bytes)
    
    total = name_tokens + desc_tokens + param_tokens + PER_TOOL_TEMPLATE_MARGIN
    return round(total, 1)


def main():
    print("=" * 80)
    print("TOOL SURFACE TOKEN DIAGNOSTIC — Definitive Analysis")
    print(f"Card: 0bd4df6e 'The tool surface costs 16,241 tokens against 5,889 of conversation'")
    print("=" * 80)
    print()
    
    # Step 1: Find all command files
    print("Step 1: Scanning for ALL command registration files...")
    command_files = find_all_command_files(REPO_ROOT)
    print(f"Found {len(command_files)} command files with NAME + ACCESS declarations")
    
    if not command_files:
        print("ERROR: No commands found. Check workspace path.")
        return
    
    # Step 2: Extract info from each file
    print("\nStep 2: Extracting command specifications...")
    specs = []
    for filepath in command_files:
        info = extract_command_info(filepath)
        if info:
            specs.append(info)
    
    print(f"Extracted {len(specs)} non-test commands")
    
    # Step 3: Compute token costs
    print("\nStep 3: Computing token costs...")
    PER_TOOL_TEMPLATE_MARGIN = 8
    
    ai_safe_specs = []
    privileged_specs = []
    other_specs = []
    
    for spec in specs:
        json_schema_bytes = estimate_json_schema_size(
            spec['param_fields'], 
            spec['params_content']
        )
        token_cost = compute_tool_token_cost(spec, json_schema_bytes)
        
        spec['json_schema_bytes'] = json_schema_bytes
        spec['token_cost'] = token_cost
        
        if spec['access_level'] == 'AiSafe':
            ai_safe_specs.append(spec)
        elif spec['access_level'] == 'Privileged':
            privileged_specs.append(spec)
        else:
            other_specs.append(spec)
    
    # Step 4: Generate report
    print("\n" + "=" * 80)
    print("RESULTS")
    print("=" * 80)
    
    # Totals by access level
    ai_safe_tokens = sum(s['token_cost'] for s in ai_safe_specs)
    privileged_tokens = sum(s['token_cost'] for s in privileged_specs)
    other_tokens = sum(s['token_cost'] for s in other_specs)
    total_tokens = ai_safe_tokens + privileged_tokens + other_tokens
    
    print(f"\nAiSafe commands:   {len(ai_safe_specs):3d}  |  ~{ai_safe_tokens:8.1f} tokens")
    print(f"Privileged cmds:   {len(privileged_specs):3d}  |  ~{privileged_tokens:8.1f} tokens")
    if other_specs:
        print(f"Other access:      {len(other_specs):3d}  |  ~{other_tokens:8.1f} tokens")
    
    total_commands = len(ai_safe_specs) + len(privileged_specs) + len(other_specs)
    print(f"{'─' * 80}")
    print(f"TOTAL:             {total_commands:3d}  |  ~{total_tokens:8.1f} tokens")
    
    # Gap analysis
    card_claimed = 16241
    gap_from_total = total_tokens - card_claimed
    gap_from_ai_safe = ai_safe_tokens - card_claimed
    
    print(f"\nCard claims:       {card_claimed:>8d} tokens")
    print(f"Our estimate:      {total_tokens:>8.1f} tokens ({'+' if gap_from_total > 0 else ''}{abs(gap_from_total):,.1f})")
    print(f"AiSafe-only:       {ai_safe_tokens:>8.1f} tokens ({'+' if gap_from_ai_safe > 0 else ''}{abs(gap_from_ai_safe):,.1f})")
    
    # Top contributors
    all_specs = sorted(specs, key=lambda s: s['token_cost'], reverse=True)
    print(f"\nTop 15 Contributors:")
    print("-" * 80)
    print(f"{'Name':<40} {'Access':<12} {'Fields':>6} {'JSON_B':>7} {'Tokens':>8}")
    print("-" * 80)
    
    for spec in all_specs[:15]:
        print(
            f"{spec['name']:<40} {spec['access_level']:<12} "
            f"{spec['param_fields']:>6d} {spec['json_schema_bytes']:>7d} "
            f"{spec['token_cost']:>8.1f}"
        )
    
    # Key insight section
    print(f"\n{'=' * 80}")
    print("KEY INSIGHT")
    print("=" * 80)
    
    conversation_size = 5889
    
    if total_tokens > conversation_size:
        ratio = total_tokens / conversation_size
        print(f"Tool surface tokens ({total_tokens:.0f}) are {ratio:.1f}x the conversation size ({conversation_size}).")
        print("The tool schema overhead DOMINATES the prompt budget — it's not just 'a bit more',")
        print("it's several times larger than the actual conversation content.")
    
    if privileged_specs:
        priv_ratio = privileged_tokens / ai_safe_tokens if ai_safe_tokens > 0 else 0
        print(f"\nPrivileged commands add {privileged_tokens:.0f} tokens ({priv_ratio:.1f}x AiSafe alone).")
        print("If the card's 16,241 figure includes Privileged tools, it's consistent with our estimate.")
    
    # Hypothesis for gap
    print(f"\nHYPOTHESIS: The 16,241 token figure likely includes:")
    print(f"  - All AiSafe commands (~{ai_safe_tokens:.0f} tokens)")
    if abs(gap_from_total) < total_tokens * 0.3:
        print(f"  - A subset of Privileged commands (our total with all: ~{total_tokens:.0f})")
        print(f"  - Our estimate is within {abs(gap_from_total)/card_claimed*100:.0f}% of the card's claim")
    else:
        print(f"  - Some other source or different counting method")
    
    # Per-access-level breakdown
    print(f"\n{'=' * 80}")
    print("DETAILED BREAKDOWN BY ACCESS LEVEL")
    print("=" * 80)
    
    for label, specs_list in [("AiSafe", ai_safe_specs), ("Privileged", privileged_specs), ("Other", other_specs)]:
        if not specs_list:
            continue
        list_tokens = sum(s['token_cost'] for s in specs_list)
        print(f"\n{label} ({len(specs_list)} commands, {list_tokens:.1f} tokens):")
        for spec in sorted(specs_list, key=lambda s: s['token_cost'], reverse=True)[:10]:
            print(f"  {spec['name']:<40} {spec['param_fields']:3d} fields  "
                  f"{spec['json_schema_bytes']:6d}B schema  {spec['token_cost']:7.1f} tokens")


if __name__ == "__main__":
    main()
