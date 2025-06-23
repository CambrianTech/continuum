#!/usr/bin/env python3
"""
Fix README diagrams by converting Unicode box-drawing to ASCII-safe characters
"""

import re

def convert_unicode_to_ascii(text):
    """Convert Unicode box-drawing characters to ASCII-safe equivalents"""
    
    # Unicode to ASCII mapping
    replacements = {
        '┌': '+',
        '┐': '+', 
        '└': '+',
        '┘': '+',
        '├': '+',
        '┤': '+',
        '┬': '+',
        '┴': '+',
        '┼': '+',
        '─': '-',
        '│': '|',
        '▼': 'v',
        '↑': '^'
    }
    
    # Apply replacements
    result = text
    for unicode_char, ascii_char in replacements.items():
        result = result.replace(unicode_char, ascii_char)
    
    return result

def fix_readme_diagrams(file_path):
    """Fix all diagrams in README file"""
    
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Convert Unicode box-drawing to ASCII
    fixed_content = convert_unicode_to_ascii(content)
    
    # Write back to file
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(fixed_content)
    
    print(f"Fixed diagrams in {file_path}")
    
    # Show what was changed
    if content != fixed_content:
        print("Changes made:")
        original_lines = content.split('\n')
        fixed_lines = fixed_content.split('\n')
        
        changes = 0
        for i, (orig, fixed) in enumerate(zip(original_lines, fixed_lines)):
            if orig != fixed:
                changes += 1
                if changes <= 5:  # Show first 5 changes
                    print(f"  Line {i+1}: {orig.strip()[:50]}... -> {fixed.strip()[:50]}...")
        
        if changes > 5:
            print(f"  ... and {changes - 5} more lines changed")
        
        print(f"Total lines changed: {changes}")
    else:
        print("No changes needed - diagrams already ASCII-safe")

if __name__ == '__main__':
    import sys
    
    if len(sys.argv) > 1:
        file_path = sys.argv[1]
    else:
        file_path = 'README.md'
    
    fix_readme_diagrams(file_path)