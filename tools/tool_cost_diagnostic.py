#!/usr/bin/env python3
"""Diagnostic: measure tool surface token cost from actual Rust source files.

Parses all AccessLevel::AiSafe command files, extracts NAME, DESCRIPTION, and
Params struct bodies, then estimates JSON Schema sizes using a heuristic that
models what schemars generates (property entries + doc-comment descriptions +
$defs for nested types/enums).

Token heuristic: char/4+1 per string segment, plus 8-token template margin
per tool — matching continuum_base's estimate_tool_schema_tokens.
"""

import json
import math
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import List, Optional


def find_aia_safe_commands(repo_root: Path) -> List[Path]:
    """Find all .rs files that define both const NAME and AccessLevel::AiSafe."""
    result = subprocess.run(
        ["grep", "-rl", "const NAME:", "--include=*.rs"],
        capture_output=True, text=True, cwd=str(repo_root)
    )
    candidate_files_raw = [Path(f) for f in result.stdout.strip().split("\n") if f]

    aisafer_files: List[Path] = []
    for f in candidate_files_raw:
        content = f.read_text(encoding="utf-8", errors="replace")
        if "AccessLevel::AiSafe" in content:
            aisafer_files.append(f)
    return sorted(aisafer_files)


def extract_name(file_path: Path) -> Optional[str]:
    """Extract const NAME value from the file."""
    content = file_path.read_text(encoding="utf-8", errors="replace")
    m = re.search(r'const\s+NAME:\s*&?\'static\s+str\s*=\s*"([^"]*)"', content)
    return m.group(1) if m else None


def extract_description(file_path: Path) -> Optional[str]:
    """Extract const DESCRIPTION value from the file."""
    content = file_path.read_text(encoding="utf-8", errors="replace")
    # DESCRIPTION spans potentially multiple lines with \n escaping
    m = re.search(r'const\s+DESCRIPTION:\s*&?\'static\s+str\s*=\s*"([^"]*(?:\\.[^"]*)*)"', content, re.DOTALL)
    if m:
        # Unescape common Rust string escapes for character counting
        raw = m.group(1)
        return raw.replace("\\n", "\n").replace('\\"', '"').replace("\\\\", "\\")
    return None


def extract_params_struct(file_path: Path) -> Optional[str]:
    """Extract the Params struct body using balanced brace matching."""
    content = file_path.read_text(encoding="utf-8", errors="replace")

    # Find lines that declare `pub struct XxxParams {` (or just `struct XxxParams`)
    for i, line in enumerate(content.split("\n")):
        m = re.search(r'(?:pub\s+)?struct\s+(\w*Params)\s*\{', line)
        if m:
            # Find the opening brace position within this content block starting at line i
            start_line = i
            # Count braces from the start of the struct to find its closing }
            brace_count = 0
            in_string = False
            escape_next = False
            lines = content.split("\n")
            for j, l in enumerate(lines[i:], start=i):
                for ch_idx, ch in enumerate(l):
                    if escape_next:
                        escape_next = False
                        continue
                    if ch == '\\':
                        escape_next = True
                        continue
                    if ch == '"':
                        in_string = not in_string
                        continue
                    if in_string:
                        continue
                    if ch == '{':
                        brace_count += 1
                    elif ch == '}':
                        brace_count -= 1
                        if brace_count == 0:
                            # Found the closing brace — return struct body
                            end_line = j + 1
                            return "\n".join(lines[start_line:end_line])
            break

    return None


def estimate_schema_size(params_body: str, file_path: Path) -> int:
    """Estimate JSON Schema character size from a Params struct body.

    Schemars generates for each field:
      "field_name": { type, description (from doc comment), ... }
    Plus $defs for nested types/enums.

    Heuristic approach: count the characters in doc comments within the struct,
    add estimated per-field schema overhead, and account for $defs from nested/enum types.
    """
    lines = params_body.split("\n")

    # Collect all doc comment text (/// ...) that precede fields
    total_doc_chars = 0
    field_count = 0
    has_nested_types = False
    has_enums = False

    i = 0
    while i < len(lines):
        line = lines[i].strip()

        # Check for doc comment
        if line.startswith("///"):
            text = line[3:].strip()
            total_doc_chars += len(text) + 4  # +4 for quotes and colon in JSON schema
            i += 1
            continue

        # Skip attribute lines (#[serde...], #[ts(...)])
        if line.startswith("#["):
            i += 1
            continue

        # Look for a field declaration: pub name: Type, or pub name : Option<Type>, etc.
        field_m = re.match(r'pub\s+(\w+)\s*:\s*(.+?),?\s*$', line)
        if field_m and not line.startswith("pub struct"):
            field_count += 1
            field_name = field_m.group(1)
            field_type_str = field_m.group(2).strip().rstrip(",")

            # Estimate schema size for this field
            # Base: "field_name": { ... } ~ 30 bytes overhead
            field_schema_size = 30 + len(field_name)

            if "Option<" in field_type_str or "skip_serializing_if" in lines[max(0, i-5):i]:
                # Optional fields add some size for the nullable/optional marker
                field_schema_size += 15

            # Check for nested types (not primitives)
            primitive_types = {"String", "bool", "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64", "usize", "isize", "Uuid"}
            for ptype in primitive_types:
                if field_type_str == ptype or f"Option<{ptype}>" in field_type_str:
                    # Small schema for primitives
                    field_schema_size += 20
                    break
            else:
                # Nested type — check if it's an enum (has variants in $defs)
                has_nested_types = True
                # Check if the file defines this nested type as an enum
                content = file_path.read_text(encoding="utf-8", errors="replace")
                type_name = re.sub(r'Option<([^>]+)>', r'\1', field_type_str).strip()
                if re.search(rf'enum\s+{re.escape(type_name)}\s*\{{', content):
                    has_enums = True

            total_doc_chars += field_schema_size

        i += 1

    # Add JSON structure overhead
    json_overhead = 40 + (field_count * 5)  # {, type: object, properties:, required: [], etc.

    # $defs overhead for nested types and enums
    defs_overhead = 0
    if has_nested_types:
        defs_overhead += 100 + (total_doc_chars // 4)  # Approximate
    if has_enums:
        defs_overhead += 200

    total_schema_chars = json_overhead + total_doc_chars + defs_overhead
    return max(total_schema_chars, 50)  # Minimum schema size


def char_to_tokens(chars: int) -> int:
    """continuum_base estimate_prompt_tokens heuristic: char/4+1."""
    return chars // 4 + 1


PER_TOOL_TEMPLATE_MARGIN = 8  # From tool_surface.rs PER_TOOL_TEMPLATE_MARGIN_TOKENS


def main():
    repo_root = Path(__file__).resolve().parent.parent

    if not repo_root.exists() or not (repo_root / ".git").exists():
        print(f"Not a repo root: {repo_root}", file=sys.stderr)
        sys.exit(1)

    files = find_aia_safe_commands(repo_root)
    print(f"Found {len(files)} AiSafe command files.\n")

    results = []
    total_name_chars = 0
    total_desc_chars = 0
    total_schema_chars = 0
    discovery_pair = False

    for f in files:
        name = extract_name(f)
        desc = extract_description(f)
        params_body = extract_params_struct(f)

        if not name or not desc:
            # Check if it's the discovery pair (commands/list, commands/help)
            fname_str = str(f)
            if "commands/list" in fname_str or "commands/help" in fname_str:
                discovery_pair = True
                # Discovery pair has no Params struct — uses empty schema
                name_chars = len(name.encode("utf-8"))
                desc_chars = len(desc.encode("utf-8")) if desc else 0
                schema_chars = 45  # {"type":"object","properties":{},"required":[]}
            continue

        name_str = name
        desc_str = desc or ""
        params_str = params_body or ""

        name_chars = len(name_str.encode("utf-8"))
        desc_chars = len(desc_str.encode("utf-8"))
        schema_chars = estimate_schema_size(params_str, f) if params_str else 45

        # Each tool: name + description + parameters (schema)
        total_tool_chars = name_chars + desc_chars + schema_chars
        tokens = char_to_tokens(total_tool_chars) + PER_TOOL_TEMPLATE_MARGIN

        results.append({
            "file": str(Path(f).resolve().relative_to(repo_root)),
            "name": name_str,
            "name_chars": name_chars,
            "desc_chars": desc_chars,
            "schema_chars": schema_chars,
            "total_chars": total_tool_chars,
            "tokens": tokens,
        })

        total_name_chars += name_chars
        total_desc_chars += desc_chars
        total_schema_chars += schema_chars

    # Discovery pair (commands/list + commands/help) — estimate separately
    disc_tokens = 0
    for f in files:
        fname_str = str(f)
        if "commands/list" in fname_str or "commands/help" in fname_str:
            name = extract_name(f)
            desc = extract_description(f)
            if name and desc:
                n_chars = len(name.encode("utf-8"))
                d_chars = len(desc.encode("utf-8"))
                s_chars = 45  # empty schema for discovery pair
                t_chars = n_chars + d_chars + s_chars
                disc_tokens += char_to_tokens(t_chars) + PER_TOOL_TEMPLATE_MARGIN

    grand_total_chars = total_name_chars + total_desc_chars + total_schema_chars
    grand_total_tokens = sum(r["tokens"] for r in results) + disc_tokens

    print(f"{'='*100}")
    print(f"PER-TOOL BREAKDOWN")
    print(f"{'='*100}")
    print(f"{'Name':<45} {'Desc':>6} {'Schema':>8} {'Total':>8} {'Tokens':>7}")
    print(f"{'-'*100}")

    for r in sorted(results, key=lambda x: -x["tokens"]):
        print(f"{r['name']:<45} {r['desc_chars']:>6} {r['schema_chars']:>8} {r['total_chars']:>8} {r['tokens']:>7}")

    print(f"\n{'='*100}")
    print(f"AGGREGATES")
    print(f"{'='*100}")
    print(f"Tools measured: {len(results)} + discovery pair ({disc_tokens}) = {len(results) + (2 if disc_tokens > 0 else 0)} total")
    print(f"\nNAME chars:      {total_name_chars:>8}  -> {char_to_tokens(total_name_chars):>6} tokens")
    print(f"DESC chars:      {total_desc_chars:>8}  -> {char_to_tokens(total_desc_chars):>6} tokens")
    print(f"SCHEMA chars:    {total_schema_chars:>8}  -> {char_to_tokens(total_schema_chars):>6} tokens")
    print(f"\nTOTAL chars:     {grand_total_chars:>8}  -> {char_to_tokens(grand_total_chars):>6} tokens (naive)")
    print(f"GRAND total:     {grand_total_tokens:>8} tokens (with per-tool margin + discovery pair)")
    print(f"\nCard figure: 16,241 tokens")
    print(f"Difference:  {16241 - grand_total_tokens:+,d}")

    # Also show what the empty-schema baseline was (for comparison)
    empty_schema_tokens = sum(
        char_to_tokens(r["name_chars"] + r["desc_chars"]) + PER_TOOL_TEMPLATE_MARGIN
        for r in results
    ) + disc_tokens
    print(f"\nEmpty-schema baseline: {empty_schema_tokens:,d} tokens")
    print(f"Schema overhead delta:  {grand_total_tokens - empty_schema_tokens:+,d}")

    # Show the "her conversation" reference
    print(f"\nReference 'her conversation': 5,889 tokens")


if __name__ == "__main__":
    main()
