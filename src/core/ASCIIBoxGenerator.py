#!/usr/bin/env python3
"""
ASCII Box Generator - Core Infrastructure Tool

Generates properly aligned ASCII art boxes for documentation and diagrams.
Designed to integrate with Continuum command system.
"""

class ASCIIBoxGenerator:
    def __init__(self):
        self.chars = {
            'horizontal': '─',
            'vertical': '│', 
            'top_left': '┌',
            'top_right': '┐',
            'bottom_left': '└', 
            'bottom_right': '┘',
            'cross': '┼',
            'tee_top': '┬',
            'tee_bottom': '┴',
            'tee_left': '├',
            'tee_right': '┤'
        }
    
    def generate_simple_box(self, lines, padding=1):
        """Generate a simple box with content"""
        if isinstance(lines, str):
            lines = [lines]
        
        # Calculate max width
        max_width = max(len(line) for line in lines) if lines else 0
        box_width = max_width + (padding * 2)
        
        # Build box
        top = self.chars['top_left'] + self.chars['horizontal'] * box_width + self.chars['top_right']
        bottom = self.chars['bottom_left'] + self.chars['horizontal'] * box_width + self.chars['bottom_right']
        
        content_lines = []
        for line in lines:
            padded_line = line.ljust(max_width)
            left_pad = ' ' * padding
            right_pad = ' ' * (box_width - len(padded_line) - padding)
            content_lines.append(f"{self.chars['vertical']}{left_pad}{padded_line}{right_pad}{self.chars['vertical']}")
        
        return [top] + content_lines + [bottom]
    
    def generate_flow_diagram(self, steps, title=None):
        """Generate a vertical flow diagram"""
        lines = []
        if title:
            lines.append(title)
            lines.append('')
        
        for i, step in enumerate(steps):
            lines.append(step)
            if i < len(steps) - 1:
                lines.append('    │')
                lines.append('    ▼')
        
        return self.generate_simple_box(lines, 2)
    
    def generate_table(self, headers, rows, padding=1):
        """Generate a table-style box"""
        # Calculate column widths
        col_widths = []
        for i, header in enumerate(headers):
            max_width = len(header)
            for row in rows:
                if i < len(row):
                    max_width = max(max_width, len(str(row[i])))
            col_widths.append(max_width + padding * 2)
        
        total_width = sum(col_widths) + len(headers) - 1
        
        # Top border
        top = self.chars['top_left'] + self.chars['horizontal'] * (total_width + 2) + self.chars['top_right']
        
        # Header row
        header_cells = []
        for i, header in enumerate(headers):
            cell = f" {header.ljust(col_widths[i] - padding)} "
            header_cells.append(cell)
        header_row = self.chars['vertical'] + ''.join(header_cells) + self.chars['vertical']
        
        # Separator
        separator = self.chars['tee_left'] + self.chars['horizontal'] * (total_width + 2) + self.chars['tee_right']
        
        # Data rows
        data_rows = []
        for row in rows:
            row_cells = []
            for i, cell in enumerate(row):
                if i < len(col_widths):
                    cell_str = f" {str(cell).ljust(col_widths[i] - padding)} "
                    row_cells.append(cell_str)
            data_rows.append(self.chars['vertical'] + ''.join(row_cells) + self.chars['vertical'])
        
        # Bottom border
        bottom = self.chars['bottom_left'] + self.chars['horizontal'] * (total_width + 2) + self.chars['bottom_right']
        
        return [top, header_row, separator] + data_rows + [bottom]
    
    def generate_command_bus_architecture(self):
        """Generate the Continuum command bus architecture diagram"""
        lines = []
        
        # Outer container
        lines.append('┌─────────────────────────────────────────┐')
        lines.append('│           Continuum Server              │')
        lines.append('│         (Orchestrator)                  │')
        lines.append('│  ┌─────────────────────────────────────┐ │')
        lines.append('│  │         Command Bus                 │ │')
        lines.append('│  │  ┌────────┐ ┌─────────┐ ┌────┐ ┌────┐ │ │')
        lines.append('│  │  │Academy │ │Screenshot│ │Chat│ │Help│ │ │')
        lines.append('│  │  │        │ │         │ │    │ │    │ │ │')
        lines.append('│  │  └────────┘ └─────────┘ └────┘ └────┘ │ │')
        lines.append('│  └─────────────────────────────────────┘ │')
        lines.append('└─────────────────────────────────────────┘')
        lines.append('         ↑                    ↑')
        lines.append('   ┌─────────┐          ┌─────────┐')
        lines.append('   │   AI    │          │ Browser │')
        lines.append('   │ Portal  │          │   UI    │')
        lines.append('   │(Python) │          │(WebApp) │')
        lines.append('   └─────────┘          └─────────┘')
        
        return lines
    
    def validate_box(self, box_lines):
        """Validate that box lines are properly aligned"""
        issues = []
        
        if len(box_lines) < 3:
            issues.append("Box must have at least 3 lines")
            return issues
        
        expected_width = len(box_lines[0])
        
        for i, line in enumerate(box_lines):
            if len(line) != expected_width:
                issues.append(f"Line {i+1}: Expected width {expected_width}, got {len(line)}")
        
        return issues
    
    def export_as_markdown(self, box_lines, language=''):
        """Export box as markdown code block"""
        return ['```' + language] + box_lines + ['```']

# Continuum command integration ready
def generate_ascii_diagram(diagram_type='simple', content=None, **kwargs):
    """
    Main function for Continuum command integration
    
    Args:
        diagram_type: 'simple', 'flow', 'table', 'command-bus'
        content: Content to diagram
        **kwargs: Additional options
    
    Returns:
        List of strings representing the ASCII diagram
    """
    generator = ASCIIBoxGenerator()
    
    if diagram_type == 'simple':
        return generator.generate_simple_box(content or ['Sample Text'])
    elif diagram_type == 'flow':
        return generator.generate_flow_diagram(content or ['Step 1', 'Step 2'])
    elif diagram_type == 'table':
        headers = kwargs.get('headers', ['Header'])
        rows = kwargs.get('rows', [['Data']])
        return generator.generate_table(headers, rows)
    elif diagram_type == 'command-bus':
        return generator.generate_command_bus_architecture()
    else:
        return generator.generate_simple_box([f"Unknown diagram type: {diagram_type}"])

if __name__ == '__main__':
    import sys
    
    generator = ASCIIBoxGenerator()
    
    if len(sys.argv) > 1:
        command = sys.argv[1]
        
        if command == 'command-bus':
            diagram = generator.generate_command_bus_architecture()
            for line in diagram:
                print(line)
        elif command == 'simple' and len(sys.argv) > 2:
            text = sys.argv[2]
            box = generator.generate_simple_box([text])
            for line in box:
                print(line)
        else:
            print("ASCII Box Generator")
            print("Usage: python ASCIIBoxGenerator.py [command-bus|simple <text>]")
    else:
        # Demo
        demo = generator.generate_simple_box(['ASCII Box Generator', 'Core Infrastructure Tool'])
        for line in demo:
            print(line)