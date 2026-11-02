#!/usr/bin/env python3
"""
Analyze tool surface token costs by parsing Rust source files directly.
Extracts NAME/DESCRIPTION pairs, estimates parameter schema sizes, and computes token costs.

Token estimation: char/4+1 heuristic + PER_TOOL_TEMPLATE_MARGIN_TOKENS (8) per tool
"""

import os
import re
import json
from pathlib import Path
from dataclasses import dataclass, field
from typing import Optional

REPO_ROOT = Path(__file__).parent.parent

@dataclass
class ToolSpec:
    name: str
    description: str
    file_path: str
    line_number: int
    param_fields: list  # List of (field_name, field_type) tuples
    estimated_schema_chars: int = 0
    
    @property
    def total_chars(self):
        """Total chars in name + description for token estimation."""
        return len(self.name) + len(self.description)
    
    @property
    def token_cost_estimate(self):
        """char/4+1 heuristic for name+description."""
        if self.total_chars == 0:
            return 1  # Minimum cost
        return int(len(self.name) / 4 + 1) + int(len(self.description) / 4 + 1)

def find_rust_files():
    """Find all .rs files in the workspace."""
    rs_files = []
    for root, dirs, files in os.walk(REPO_ROOT):
        # Skip hidden directories and target/
        dirs[:] = [d for d in dirs if not d.startswith('.') and d != 'target']
        for f in files:
            if f.endswith('.rs'):
                rs_files.append(Path(root) / f)
    return sorted(rs_files)

def extract_struct_name(lines, struct_line_idx):
    """Extract the struct name from a line containing 'struct Xxx'."""
    # Look backwards from current line for struct declaration
    for i in range(struct_line_idx, max(0, struct_line_idx - 50), -1):
        match = re.search(r'struct\s+(\w+)', lines[i])
        if match:
            return match.group(1)
    return None

def extract_params_struct(file_path, start_line=0):
    """Find Params structs in a file and estimate their schema size."""
    try:
        content = Path(file_path).read_text()
        lines = content.split('\n')
        
        # Find all struct declarations that look like params
        param_patterns = []
        for i, line in enumerate(lines):
            if i < start_line:
                continue
            # Match "struct Params" or "struct XxxParams" 
            match = re.search(r'struct\s+(\w*Params?\w*)', line)
            if match and i > 5:  # Skip first few lines (module declarations etc)
                struct_name = match.group(1)
                param_patterns.append((i, struct_name))
        
        total_schema_chars = 0
        param_fields = []
        
        for struct_line_idx, struct_name in param_patterns[:2]:  # Usually one Params struct
            # Find the struct body (between { and })
            brace_depth = 0
            start_brace = -1
            end_brace = -1
            
            for j in range(struct_line_idx, min(len(lines), struct_line_idx + 200)):
                line = lines[j]
                if '{' in line:
                    brace_depth += line.count('{')
                    if start_brace == -1:
                        start_brace = j
                
                if '}' in line:
                    brace_depth -= line.count('}')
                    if brace_depth <= 0 and start_brace != -1:
                        end_brace = j
                        break
            
            if start_brace != -1 and end_brace != -1:
                # Extract field definitions from struct body
                for k in range(start_brace + 1, min(end_brace, start_brace + 50)):
                    line = lines[k].strip()
                    # Match "field_name: Type," or "#[serde(...)]" followed by field
                    if ':' in line and not line.startswith('//') and not line.startswith('fn '):
                        field_match = re.match(r'(\w+)\s*:\s*(\w+)', line)
                        if field_match:
                            param_fields.append((field_match.group(1), field_match.group(2)))
                
                # Estimate schema size based on fields
                for field_name, field_type in param_fields:
                    # Base estimate per field
                    base_chars = len(field_name) + 4 + len(field_type)
                    
                    # Type-specific adjustments
                    type_sizes = {
                        'String': 100,      # String params are large
                        '&str': 80,
                        'i32': 5,
                        'u32': 5,
                        'i64': 7,
                        'u64': 7,
                        'f64': 6,
                        'bool': 5,
                        'Option': 30,       # Optional fields add schema complexity
                        'Vec': 20,          # Arrays add overhead
                    }
                    
                    for type_prefix, size in type_sizes.items():
                        if field_type.startswith(type_prefix):
                            base_chars = max(base_chars, size)
                            break
                    
                    total_schema_chars += base_chars + 50  # Add overhead for JSON key/type/etc
        
        return param_fields, total_schema_chars
    
    except Exception:
        return [], 0

def parse_name_description_pairs(rs_files):
    """Extract NAME/DESCRIPTION pairs from Rust source files."""
    tools = []
    
    for file_path in rs_files:
        try:
            content = Path(file_path).read_text()
            lines = content.split('\n')
            
            # Find all const NAME declarations
            for i, line in enumerate(lines):
                name_match = re.search(r'const\s+NAME:\s*&\'static\s+str\s*=\s*"([^"]*)"', line)
                
                if not name_match:
                    continue
                
                tool_name = name_match.group(1)
                
                # Find the struct name (look backwards up to 50 lines)
                struct_name = None
                for j in range(i, max(0, i - 50), -1):
                    if 'struct' in lines[j]:
                        struct_match = re.search(r'struct\s+(\w+)', lines[j])
                        if struct_match:
                            struct_name = struct_match.group(1)
                            break
                
                # Find corresponding DESCRIPTION (look within ±20 lines)
                description = ""
                desc_start = max(0, i - 30)
                desc_end = min(len(lines), i + 30)
                
                for j in range(desc_start, desc_end):
                    if 'const DESCRIPTION' in lines[j]:
                        # Extract description value (may be multi-line string literal)
                        desc_line = lines[j]
                        
                        # Check for inline description
                        inline_match = re.search(r'DESCRIPTION:\s*&\'static\s+str\s*=\s*"(.*)"', desc_line)
                        if inline_match:
                            description = inline_match.group(1)
                            break
                        
                        # Multi-line string literal (starts with " but doesn't end on same line)
                        if desc_line.strip().endswith('"') and '"' in desc_line.split('="')[1] if '="' in desc_line else False:
                            # Single-line, already handled above
                            pass
                        elif '="' in desc_line:
                            # Multi-line: collect until closing quote
                            partial = desc_line.split('="', 1)[1] if '="' in desc_line else ""
                            
                            # Continue reading lines until we find closing "
                            for k in range(j + 1, min(len(lines), j + 50)):
                                partial += lines[k]
                                if '"' in lines[k]:
                                    description = partial.split('"')[0]
                                    break
                        
                        break
                
                # Extract parameter info
                param_fields, schema_chars = extract_params_struct(file_path, i - 20)
                
                tools.append(ToolSpec(
                    name=tool_name,
                    description=description,
                    file_path=str(file_path),
                    line_number=i + 1,
                    param_fields=param_fields,
                    estimated_schema_chars=schema_chars
                ))
        
        except Exception as e:
            # Skip files that can't be read (binary, permission issues)
            continue
    
    return tools

def compute_token_costs(tools):
    """Compute token costs using char/4+1 heuristic + 8-token margin per tool."""
    PER_TOOL_TEMPLATE_MARGIN = 8
    
    total_name_tokens = sum(t.token_cost_estimate for t in tools)
    total_schema_tokens = sum(
        int(t.estimated_schema_chars / 4 + 1) if t.estimated_schema_chars > 0 else 0
        for t in tools
    )
    template_overhead = len(tools) * PER_TOOL_TEMPLATE_MARGIN
    
    # Discovery pair (commands/list + commands/help) - typically ~2 tools with specific margins
    discovery_pair_tokens = 200  # Approximate for the two discovery tools
    
    grand_total = total_name_tokens + total_schema_tokens + template_overhead + discovery_pair_tokens
    
    return {
        'name_tokens': total_name_tokens,
        'schema_tokens': total_schema_tokens,
        'template_margin': template_overhead,
        'discovery_pair': discovery_pair_tokens,
        'grand_total': grand_total,
        'tool_count': len(tools)
    }

def print_report(costs, tools=None):
    """Print a summary report."""
    print("\n" + "=" * 80)
    print("TOOL SURFACE TOKEN COST ANALYSIS")
    print("=" * 80)
    print(f"\nTotal tools found: {costs['tool_count']}")
    print(f"\nToken cost breakdown:")
    print(f"  Name/Description tokens (char/4+1): {costs['name_tokens']:>6}")
    print(f"  Schema tokens (estimated):          {costs['schema_tokens']:>6}")
    print(f"  Template margin ({8} per tool × {costs['tool_count']} tools): {costs['template_margin']:>6}")
    print(f"  Discovery pair overhead:             {costs['discovery_pair']:>6}")
    print(f"  {'─' * 40}")
    print(f"  GRAND TOTAL:                         {costs['grand_total']:>6} tokens")

    # Top 10 tools by name+description cost
    if tools:
        top_tools = sorted(
            [(t.name, t.token_cost_estimate) for t in tools],
            key=lambda x: x[1],
            reverse=True
        )[:10]

        print(f"\nTop 10 tools by description size:")
        for name, cost in top_tools:
            desc_preview = f'"{name}" - {cost} tokens'
            if len(desc_preview) > 70:
                desc_preview = desc_preview[:67] + "..."
            print(f"  {desc_preview}")

def main():
    print("Finding Rust source files...")
    rs_files = find_rust_files()
    print(f"Found {len(rs_files)} .rs files\n")
    
    print("Extracting NAME/DESCRIPTION pairs...")
    tools = parse_name_description_pairs(rs_files)
    print(f"Found {len(tools)} tool specifications\n")
    
    if not tools:
        print("ERROR: No tools found. Check file paths and patterns.")
        return
    
    costs = compute_token_costs(tools)
    print_report(costs, tools)
