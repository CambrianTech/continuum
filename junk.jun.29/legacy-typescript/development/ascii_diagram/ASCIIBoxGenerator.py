#!/usr/bin/env python3
"""
ASCII Box Generator - Core Infrastructure Tool

Generates properly aligned ASCII art boxes for documentation and diagrams.
Designed to integrate with Continuum command system.
"""

class ASCIIBoxGenerator:
    def __init__(self, ascii_safe=False):
        if ascii_safe:
            # ASCII-safe characters for GitHub compatibility
            self.chars = {
                'horizontal': '-',
                'vertical': '|',
                'top_left': '+',
                'top_right': '+',
                'bottom_left': '+',
                'bottom_right': '+',
                'cross': '+',
                'tee_top': '+',
                'tee_bottom': '+',
                'tee_left': '+',
                'tee_right': '+'
            }
        else:
            # Unicode box-drawing characters
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
                lines.append('    |')
                lines.append('    v')
        
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
    
    def _make_border(self, width):
        """Create a border line of specified width"""
        return self.chars['top_left'] + self.chars['horizontal'] * (width - 2) + self.chars['top_right']
    
    def _make_content_line(self, content, width):
        """Create a content line with proper centering"""
        content_len = len(content)
        if content_len >= width - 2:
            return self.chars['vertical'] + content[:width-2] + self.chars['vertical']
        
        padding = width - content_len - 2
        left_pad = padding // 2
        right_pad = padding - left_pad
        return self.chars['vertical'] + ' ' * left_pad + content + ' ' * right_pad + self.chars['vertical']
    
    def _make_nested_container(self, outer_width, inner_width, inner_content):
        """Create a nested container with proper centering"""
        inner_line = self._make_content_line(inner_content, inner_width)
        return self._make_content_line(inner_line, outer_width)
    
    def _create_command_row(self, commands):
        """Create a row of command boxes with proper spacing"""
        boxes = []
        contents = []
        empties = []
        
        for cmd in commands:
            width = len(cmd) + 2  # content + 2 for borders
            box_top = '+' + '-' * len(cmd) + '+'
            box_content = '|' + cmd + '|'
            box_empty = '|' + ' ' * len(cmd) + '|'
            
            boxes.append(box_top)
            contents.append(box_content)
            empties.append(box_empty)
        
        return ' '.join(boxes), ' '.join(contents), ' '.join(empties)
    
    def generate_command_bus_architecture_safe(self):
        """Generate ASCII-safe command bus architecture with smart spacing"""
        commands = ['Academy', 'Screenshot', 'Chat', 'Help']
        
        # Calculate dimensions based on content
        cmd_boxes, cmd_content, cmd_empty = self._create_command_row(commands)
        cmd_area_width = len(cmd_boxes)
        inner_width = cmd_area_width + 4  # command area + padding
        outer_width = inner_width + 4     # inner container + padding
        
        lines = []
        
        # Outer container
        lines.append(self._make_border(outer_width))
        lines.append(self._make_content_line('Continuum Server', outer_width))
        lines.append(self._make_content_line('(Orchestrator)', outer_width))
        
        # Inner container
        inner_border = self._make_border(inner_width)
        lines.append(self._make_content_line(inner_border, outer_width))
        lines.append(self._make_nested_container(outer_width, inner_width, 'Command Bus'))
        
        # Command boxes
        lines.append(self._make_nested_container(outer_width, inner_width, cmd_boxes))
        lines.append(self._make_nested_container(outer_width, inner_width, cmd_content))
        lines.append(self._make_nested_container(outer_width, inner_width, cmd_empty))
        lines.append(self._make_nested_container(outer_width, inner_width, cmd_boxes))
        
        # Close containers
        lines.append(self._make_content_line(inner_border, outer_width))
        lines.append(self._make_border(outer_width))
        
        # Connection arrows - calculate center positions
        center = outer_width // 2
        left_pos = center - 15
        right_pos = center + 5
        
        arrow_line = ' ' * left_pos + '^' + ' ' * 18 + '^'
        lines.append(arrow_line)
        
        # Client boxes - positioned under arrows
        client_line = ' ' * (left_pos - 4) + '+---------+' + ' ' * 8 + '+---------+'
        lines.append(client_line)
        
        ai_line = ' ' * (left_pos - 4) + '|   AI    |' + ' ' * 8 + '| Browser |'
        lines.append(ai_line)
        
        portal_line = ' ' * (left_pos - 4) + '| Portal  |' + ' ' * 8 + '|   UI    |'
        lines.append(portal_line)
        
        lang_line = ' ' * (left_pos - 4) + '|(Python) |' + ' ' * 8 + '|(WebApp) |'
        lines.append(lang_line)
        
        bottom_line = ' ' * (left_pos - 4) + '+---------+' + ' ' * 8 + '+---------+'
        lines.append(bottom_line)
        
        return lines
    
    def generate_org_chart(self, title, levels):
        """Generate an organizational chart with proper connection calculations
        
        Args:
            title: Top level title
            levels: List of lists, each representing a level of the org chart
        """
        lines = []
        
        # Calculate widths for each level
        level_widths = []
        for level in levels:
            level_width = sum(len(item) + 4 for item in level) + (len(level) - 1) * 3  # boxes + spacing
            level_widths.append(level_width)
        
        # Find maximum width needed
        max_width = max([len(title) + 4] + level_widths)
        
        # Top box (title)
        title_box = self.generate_simple_box([title])
        title_start = (max_width - len(title_box[0])) // 2
        
        for line in title_box:
            lines.append(' ' * title_start + line)
        
        # Add connection line down from title
        title_center = title_start + len(title_box[0]) // 2
        lines.append(' ' * title_center + '|')
        
        # Process each level
        for level_idx, level in enumerate(levels):
            if not level:
                continue
                
            # Calculate positions for this level's boxes
            level_width = level_widths[level_idx]
            level_start = (max_width - level_width) // 2
            
            # Create boxes for this level
            boxes = []
            positions = []
            current_pos = level_start
            
            for item in level:
                box = self.generate_simple_box([item])
                boxes.append(box)
                positions.append(current_pos)
                current_pos += len(box[0]) + 3  # box width + spacing
            
            # If multiple boxes, add horizontal connection line
            if len(boxes) > 1:
                left_center = positions[0] + len(boxes[0][0]) // 2
                right_center = positions[-1] + len(boxes[-1][0]) // 2
                
                # Horizontal line connecting all boxes
                h_line = ' ' * left_center + '+' + '-' * (right_center - left_center - 1) + '+'
                lines.append(h_line)
                
                # Vertical drops to each box
                v_line = ''
                for i, pos in enumerate(positions):
                    box_center = pos + len(boxes[i][0]) // 2
                    if i == 0:
                        v_line = ' ' * box_center + '|'
                    else:
                        while len(v_line) < box_center:
                            v_line += ' '
                        v_line += '|'
                lines.append(v_line)
            else:
                # Single box, just a vertical line
                box_center = positions[0] + len(boxes[0][0]) // 2
                lines.append(' ' * box_center + '|')
            
            # Add the boxes themselves
            max_box_height = max(len(box) for box in boxes)
            for row in range(max_box_height):
                line = ''
                for i, box in enumerate(boxes):
                    pos = positions[i]
                    if row < len(box):
                        box_line = box[row]
                    else:
                        box_line = ' ' * len(box[0])  # Empty line same width as box
                    
                    while len(line) < pos:
                        line += ' '
                    line += box_line
                lines.append(line)
        
        return lines
    
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
def generate_ascii_diagram(diagram_type='simple', content=None, ascii_safe=False, **kwargs):
    """
    Main function for Continuum command integration
    
    Args:
        diagram_type: 'simple', 'flow', 'table', 'command-bus', 'org-chart'
        content: Content to diagram
        ascii_safe: Use ASCII-safe characters for GitHub compatibility
        **kwargs: Additional options
    
    Returns:
        List of strings representing the ASCII diagram
    """
    generator = ASCIIBoxGenerator(ascii_safe=ascii_safe)
    
    if diagram_type == 'simple':
        return generator.generate_simple_box(content or ['Sample Text'])
    elif diagram_type == 'flow':
        return generator.generate_flow_diagram(content or ['Step 1', 'Step 2'])
    elif diagram_type == 'table':
        headers = kwargs.get('headers', ['Header'])
        rows = kwargs.get('rows', [['Data']])
        return generator.generate_table(headers, rows)
    elif diagram_type == 'command-bus':
        if ascii_safe:
            return generator.generate_command_bus_architecture_safe()
        else:
            return generator.generate_command_bus_architecture()
    elif diagram_type == 'org-chart':
        title = kwargs.get('title', 'Organization')
        levels = kwargs.get('levels', [['Manager'], ['Dev1', 'Dev2']])
        return generator.generate_org_chart(title, levels)
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
        elif command == 'command-bus-safe':
            generator_safe = ASCIIBoxGenerator(ascii_safe=True)
            diagram = generator_safe.generate_command_bus_architecture_safe()
            for line in diagram:
                print(line)
        elif command == 'simple' and len(sys.argv) > 2:
            text = sys.argv[2]
            box = generator.generate_simple_box([text])
            for line in box:
                print(line)
        else:
            print("ASCII Box Generator")
            print("Usage: python ASCIIBoxGenerator.py [command-bus|command-bus-safe|simple <text>]")
    else:
        # Demo
        demo = generator.generate_simple_box(['ASCII Box Generator', 'Core Infrastructure Tool'])
        for line in demo:
            print(line)