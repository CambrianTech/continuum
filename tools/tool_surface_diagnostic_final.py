#!/usr/bin/env python3
"""
Diagnostic for card 0bd4df6e: tool surface token cost analysis.

Extracts all command names from Rust source, reads their descriptions,
estimates token costs using the char/4+1 heuristic from tool_surface.rs,
and categorizes by subtree path to identify where the gap between
measured and claimed totals comes from.
"""
import subprocess
import sys
import re
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent


def run_grep(pattern):
    """Run grep on core/ using fixed-string matching, return matching lines."""
    result = subprocess.run(
        ["grep", "-rnF", pattern, "core/", "--include=*.rs"],
        cwd=str(REPO_ROOT), capture_output=True, text=True
    )
    if result.returncode != 0:
        return []
    out = result.stdout.strip()
    return out.split('\n') if out else []


def get_description(file_path, line_num):
    """Read surrounding lines to find DESCRIPTION for a command.

    Handles both single-line and multi-line (backslash-continued) string literals.
    """
    try:
        with open(file_path) as f:
            lines = f.readlines()
    except Exception:
        return ""

    # Look for DESCRIPTION on nearby lines (within 20 before or up to 100 after NAME)
    start = max(0, line_num - 20)
    end = min(len(lines), line_num + 100)  # Extended past NAME — DESCRIPTION often follows

    desc_start_line = None
    for i in range(start, end):
        stripped = lines[i].strip()
        if 'DESCRIPTION:' in stripped:
            desc_start_line = i
            break

    if desc_start_line is None:
        return ""

    # From the DESCRIPTION line onward, collect text inside quotes.
    # Handles both single-line and multi-line (backslash-continued) strings.
    parts = []
    current_line = desc_start_line

    while True:
        line_content = lines[current_line]

        # Look for opening quote on this line
        q_open = line_content.find('"')
        if q_open != -1:
            # Check if there's content between DESCRIPTION: and the first "
            desc_idx = line_content.find('DESCRIPTION:')
            if desc_idx != -1:
                before_quote = line_content[desc_idx + len('DESCRIPTION'):]
                before_q = before_quote.lstrip().lstrip('&').lstrip()
                q_open2 = before_q.find('"')
                if q_open2 == 0 and not before_q.strip():
                    # The " starts immediately — nothing to capture from this segment
                    pass
            
            rest = line_content[q_open + 1:]  # After the opening quote

            while True:
                r_stripped = rest.rstrip()
                if '\\' in r_stripped and r_stripped.endswith('\\'):
                    # Backslash continuation: grab content before backslash
                    parts.append(r_stripped[:-1].rstrip())
                    current_line += 1
                    if current_line >= len(lines):
                        return ' '.join(parts)
                    rest = lines[current_line]
                elif '"' in r_stripped:
                    # Found closing quote — grab everything before it
                    before_close = r_stripped[:r_stripped.index('"')]
                    parts.append(before_close.rstrip())
                    return ' '.join(parts)
                else:
                    # Continuation line with no closing quote yet
                    current_line += 1
                    if current_line >= len(lines):
                        return ' '.join(parts) if parts else ""
                    rest = lines[current_line]
        elif current_line < end - 1:
            # No quote on this DESCRIPTION marker line; try next
            current_line += 1
        else:
            break


def estimate_tokens(text):
    """Estimate token count using char/4+1 heuristic from tool_surface.rs."""
    if not text:
        return 0
    return len(text) // 4 + 1


def get_subtree(name, file_path):
    """Determine which subtree a command belongs to based on its name and path."""
    # hands_surface subtrees from tool_surface.rs line ~400
    HAND_SUBTREES = [
        'focus/', 'room/', 'chat/', 'code/', 'work/',
        'git/', 'cargo/', 'tool/', 'commands/', 'web/'
    ]

    for subtree in HAND_SUBTREES:
        if name.startswith(subtree):
            return f"hand/{subtree}"

    # Check file path as fallback
    parts = file_path.split('/')
    crate_name = "unknown"
    for part in parts:
        if part.startswith('continuum'):
            crate_name = part
            break

    return f"{crate_name}/{name}"


def main():
    print("=" * 70)
    print("Tool Surface Token Cost Diagnostic")
    print(f"Card: 0bd4df6e — 'The tool surface costs 16,241 tokens'")
    print("=" * 70)

    # Get all commands using fixed-string grep (avoids regex escaping issues)
    name_lines = run_grep("const NAME: &'static str = \"")

    print(f"\nTotal lines matching 'const NAME': {len(name_lines)}\n")

    if not name_lines or (len(name_lines) == 1 and not name_lines[0].strip()):
        print("ERROR: No commands found. Check grep pattern.")
        return 1

    # Debug: show first 3 lines to verify format
    for line in name_lines[:3]:
        print(f"  Sample: {line}")
    print()

    # Parse each command
    commands = []
    skipped_constants = 0
    parse_errors = 0

    for line in name_lines:
        if not line.strip():
            continue

        parts = line.split(':', 2)
        if len(parts) < 3:
            parse_errors += 1
            continue

        file_path = parts[0]
        try:
            line_num = int(parts[1]) - 1  # Convert to 0-indexed
        except ValueError:
            parse_errors += 1
            continue

        name_part = parts[2].strip()

        # Extract the command name from between quotes
        name_match = re.search(r'=\s*"([^"]+)"', name_part)
        if not name_match:
            # This is a constant reference like COMMAND_OPEN, not a string literal
            skipped_constants += 1
            continue

        name = name_match.group(1)

        desc = get_description(file_path, line_num)
        subtree = get_subtree(name, file_path)

        commands.append({
            'name': name,
            'description': desc,
            'file': file_path,
            'line': line_num + 1,
            'subtree': subtree,
            'name_tokens': estimate_tokens(name),
            'desc_tokens': estimate_tokens(desc),
        })

    print(f"Commands parsed: {len(commands)}")
    print(f"Skipped (constants like COMMAND_OPEN): {skipped_constants}")
    print(f"Parse errors: {parse_errors}\n")

    # Token cost summary
    total_name_chars = sum(len(c['name']) for c in commands)
    total_desc_chars = sum(len(c['description']) for c in commands)
    total_name_tokens = sum(c['name_tokens'] for c in commands)
    total_desc_tokens = sum(c['desc_tokens'] for c in commands)

    print("-" * 70)
    print("Token Cost Summary (char/4+1 heuristic)")
    print("-" * 70)
    print(f"Total names:     {total_name_chars:>8} chars → {total_name_tokens:>6} tokens")
    print(f"Total desc:      {total_desc_chars:>8} chars → {total_desc_tokens:>6} tokens")
    combined_chars = total_name_chars + total_desc_chars
    combined_tokens = total_name_tokens + total_desc_tokens
    print(f"Combined:        {combined_chars:>8} chars → {combined_tokens:>6} tokens")

    # Per-command detail
    per_command = []
    for c in commands:
        cost = c['name_tokens'] + c['desc_tokens']
        per_command.append((c, cost))
    per_command.sort(key=lambda x: x[1], reverse=True)

    total_schema_overhead_est = len(commands) * 8  # Template margin from earlier findings
    print(f"\nPer-command template margin: {len(commands)} × 8 = {total_schema_overhead_est} tokens")

    grand_total_name_desc = sum(x[1] for x in per_command) + total_schema_overhead_est
    print(f"Grand total (name+desc+margin): ~{grand_total_name_desc} tokens")

    # Compare to card figure
    gap = 16241 - grand_total_name_desc
    print(f"\nCard claimed:        16,241 tokens")
    print(f"Our estimate:       ~{grand_total_name_desc} tokens")
    print(f"Gap (or surplus):   {'+' if gap > 0 else ''}{gap} tokens")

    # Breakdown by subtree
    print("\n" + "-" * 70)
    print("Breakdown by Subtree")
    print("-" * 70)

    subtrees = {}
    for c, cost in per_command:
        st = c['subtree']
        if st not in subtrees:
            subtrees[st] = {'count': 0, 'chars': 0, 'tokens': 0}
        subtrees[st]['count'] += 1
        subtrees[st]['chars'] += len(c['name']) + len(c['description'])
        subtrees[st]['tokens'] += cost

    # Sort by token count descending
    for st in sorted(subtrees, key=lambda x: subtrees[x]['tokens'], reverse=True):
        info = subtrees[st]
        print(f"  {st:<30s} {info['count']:>3d} tools  {info['chars']:>7d} chars  ~{info['tokens']:>5d} tokens")

    # Top 10 by token cost
    print("\n" + "-" * 70)
    print("Top 10 Tools by Token Cost (name+description)")
    print("-" * 70)
    for c, cost in per_command[:10]:
        desc_preview = c['description'][:50] if len(c['description']) > 50 else c['description']
        print(f"  {cost:>5d} tokens  {c['name']:30s} ({desc_preview})")

    # Bottom 10 by token cost
    print("\nBottom 10 Tools by Token Cost (name+description)")
    print("-" * 70)
    for c, cost in per_command[-10:]:
        desc_preview = c['description'][:50] if len(c['description']) > 50 else c['description']
        print(f"  {cost:>5d} tokens  {c['name']:30s} ({desc_preview})")

    # Hypothesis section
    print("\n" + "=" * 70)
    print("HYPOTHESIS ANALYSIS")
    print("=" * 70)
    print(f"Our estimate for ALL commands: ~{grand_total_name_desc} tokens")
    if gap > 0:
        print(f"This is {gap} tokens below the card's 16,241.")
        print("\nPossible explanations for remaining gap:")
        print("  1. Schema overhead (ToolInputSchema JSON) is larger than our")
        print("     char/4+1 heuristic — actual serde_json serialization adds more bytes")
        print("  2. Wire dialect transformation in tool_dialect.rs modifies specs before")
        print("     serialization, adding metadata")
        print("  3. Prompt template overhead (role prefixes, separators) not captured here")
        print(f"  4. {skipped_constants} commands with constant-name references (COMMAND_*) were skipped")
    else:
        print("Our estimate exceeds the card figure — our heuristic may overestimate.")

    return 0


if __name__ == '__main__':
    sys.exit(main())
