#!/usr/bin/env python3
"""
Diagnostic: measure per-tool token cost for persona tool surfaces.
Parses Rust source files directly to extract NativeToolSpec definitions,
serializes them as JSON, and reports character/token counts.

Run from workspace root:
  python3 tools/tool_surface_cost_diagnostic.py
"""

import json
import re
import subprocess
import sys


def run(cmd, cwd=None):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd or ".")
    return result.stdout.strip()


# Step 1: Find all native tool spec definitions in persona_tools.rs
PERSONA_TOOLS_PATH = "core/continuum-core/src/test_env/mind_integration/registry/persona_tools.rs"

content = open(PERSONA_TOOLS_PATH).read()

# Look for the native_tool_specs function to find where specs are defined
func_match = re.search(r'fn native_tool_specs\(\)', content)
if not func_match:
    print("ERROR: Could not find native_tool_specs function")
    sys.exit(1)

# Extract the function body (roughly - find between braces)
start = func_match.start()
brace_count = 0
in_body = False
body_start = None
for i, ch in enumerate(content[start:], start):
    if ch == '{':
        brace_count += 1
        in_body = True
        body_start = i + 1
    elif ch == '}':
        brace_count -= 1
        if in_body and brace_count == 0:
            body_end = i
            break

func_body = content[body_start:body_end]

# Step 2: Extract tool names and their descriptions from the function body
# Look for patterns like NativeToolSpec { name: "...", description: "...", input_schema: ... }
spec_pattern = re.compile(
    r'NativeToolSpec\s*\{\s*name:\s*"([^"]+)"[^}]*description:\s*"([^"]*)"[^}]*input_schema:\s*(\{.*?\})',
    re.DOTALL
)

specs_found = []
for match in spec_pattern.finditer(func_body):
    name = match.group(1)
    desc = match.group(2)
    schema_str = match.group(3)
    
    # Try to parse the schema as JSON (it might be a Rust struct literal, not valid JSON)
    try:
        schema = json.loads(schema_str.replace("'", '"'))
        spec_json = {
            "name": name,
            "description": desc,
            "input_schema": schema
        }
        specs_found.append(spec_json)
    except json.JSONDecodeError:
        # Rust struct literal isn't valid JSON — just record what we can
        specs_found.append({
            "name": name,
            "description": desc[:100],  # truncated
            "_raw_schema_len": len(schema_str),
            "_note": "Rust struct (not JSON)"
        })

print("=" * 72)
print("TOOL SURFACE COST DIAGNOSTIC")
print(f"Source: {PERSONA_TOOLS_PATH}")
print(f"Native tools found: {len(specs_found)}")
print("=" * 72)

# Step 3: For each spec, measure JSON serialization size
total_chars = 0
for i, spec in enumerate(specs_found):
    name = spec.get("name", f"tool_{i}")
    
    if "_raw_schema_len" in spec:
        # Can't serialize — use raw schema length as proxy
        chars = len(json.dumps({"name": name, "description": spec["description"]})) + spec["_raw_schema_len"]
        print(f"\n{name}:")
        print(f"  Description (first 100): {spec['description'][:100]}...")
        print(f"  Schema raw length: {spec['_raw_schema_len']} chars")
        print(f"  Estimated total: ~{chars} chars (~{chars/4:.0f} tokens)")
    else:
        json_str = json.dumps(spec)
        chars = len(json_str)
        total_chars += chars
        
        print(f"\n{i+1}. {name}")
        print(f"   Description ({len(spec['description'])} chars):")
        # Show first 120 chars of description
        desc_preview = spec['description'][:120] + ("..." if len(spec['description']) > 120 else "")
        print(f"     {desc_preview}")
        
        schema_str = json.dumps(spec.get("input_schema", {}), indent=2)
        print(f"   Schema ({len(schema_str)} chars):")
        # Show first 8 lines of schema
        for line in schema_str.split('\n')[:8]:
            print(f"     {line}")
        if '\n' in schema_str:
            print(f"     ... ({schema_str.count(chr(10))} total lines)")
        
        print(f"   Total JSON: {chars} chars (~{chars/4:.0f} tokens)")

# Step 4: Summary
est_tokens = total_chars / 4.0
print("\n" + "=" * 72)
print("SUMMARY")
print("=" * 72)
print(f"Total native tools serialized: {total_chars} chars (~{est_tokens:.0f} tokens)")
print()

# Step 5: Also check registry tool costs if available
REGISTRY_PATH = "core/continuum-cognition/src/cognition/tool_surface.rs"
registry_content = open(REGISTRY_PATH).read()

# Look for where registry tools are loaded and measured
if "registry_tool_tokens" in registry_content or "tool_surface_tokens_of" in registry_content:
    print("\nRegistry tool measurement locations:")
    for i, line in enumerate(registry_content.split('\n'), 1):
        if any(kw in line for kw in ['registry_tool_tokens', 'tool_surface_tokens_of', 'estimate_tool_schema']):
            print(f"  {REGISTRY_PATH}:{i}: {line.strip()}")

# Step 6: Check tool_dialect.rs for serialization logic
TOOL_DIALECT_PATH = "core/continuum-engine/src/ai/tool_dialect.rs"
dialect_content = open(TOOL_DIALECT_PATH).read()

print("\nSerialization logic locations:")
for i, line in enumerate(dialect_content.split('\n'), 1):
    if any(kw in line for kw in ['offer_style_for', 'serialize', 'to_string', 'serde_json']):
        print(f"  {TOOL_DIALECT_PATH}:{i}: {line.strip()}")

print("\nNote: Token estimation uses ~4 chars per token (rough approximation).")
print("Actual token counts may vary based on the tokenizer used.")
