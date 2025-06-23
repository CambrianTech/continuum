#!/usr/bin/env node

/**
 * ASCII Box Generator Tool
 * 
 * Generates properly aligned ASCII art boxes for documentation
 * Usage: node scripts/ascii-box-generator.js [options]
 */

class ASCIIBoxGenerator {
  constructor() {
    this.chars = {
      // Box drawing characters
      horizontal: '─',
      vertical: '│',
      topLeft: '┌',
      topRight: '┐', 
      bottomLeft: '└',
      bottomRight: '┘',
      cross: '┼',
      teeTop: '┬',
      teeBottom: '┴',
      teeLeft: '├',
      teeRight: '┤'
    };
  }

  /**
   * Generate a simple box with content
   */
  generateSimpleBox(lines, padding = 1) {
    if (!Array.isArray(lines)) lines = [lines];
    
    // Calculate max width
    const maxWidth = Math.max(...lines.map(line => line.length));
    const boxWidth = maxWidth + (padding * 2);
    
    const top = this.chars.topLeft + this.chars.horizontal.repeat(boxWidth) + this.chars.topRight;
    const bottom = this.chars.bottomLeft + this.chars.horizontal.repeat(boxWidth) + this.chars.bottomRight;
    
    const contentLines = lines.map(line => {
      const paddedLine = line.padEnd(maxWidth);
      const leftPad = ' '.repeat(padding);
      const rightPad = ' '.repeat(boxWidth - paddedLine.length - padding);
      return this.chars.vertical + leftPad + paddedLine + rightPad + this.chars.vertical;
    });
    
    return [top, ...contentLines, bottom];
  }

  /**
   * Generate nested boxes (box within box)
   */
  generateNestedBox(outerLines, innerBoxes, outerPadding = 2) {
    // Start with outer box structure
    const result = [...outerLines];
    
    // Insert inner boxes at specified positions
    innerBoxes.forEach(({ position, box, indent = 2 }) => {
      const indentStr = ' '.repeat(indent);
      box.forEach((boxLine, index) => {
        const fullLine = this.chars.vertical + indentStr + boxLine + ' '.repeat(
          result[0].length - boxLine.length - indent - 2
        ) + this.chars.vertical;
        result.splice(position + index, 0, fullLine);
      });
    });
    
    return result;
  }

  /**
   * Generate a flow diagram box
   */
  generateFlowBox(title, items, arrows = true) {
    const lines = [title, ''];
    
    items.forEach((item, index) => {
      lines.push(item);
      if (arrows && index < items.length - 1) {
        lines.push('    │');
        lines.push('    ▼');
      }
    });
    
    return this.generateSimpleBox(lines, 2);
  }

  /**
   * Generate table-style box
   */
  generateTable(headers, rows, columnWidths = null) {
    if (!columnWidths) {
      columnWidths = headers.map((header, i) => 
        Math.max(header.length, ...rows.map(row => (row[i] || '').length))
      );
    }
    
    const totalWidth = columnWidths.reduce((sum, width) => sum + width, 0) + columnWidths.length - 1;
    
    // Top border
    const top = this.chars.topLeft + this.chars.horizontal.repeat(totalWidth + 2) + this.chars.topRight;
    
    // Header row
    const headerRow = this.chars.vertical + ' ' + 
      headers.map((header, i) => header.padEnd(columnWidths[i])).join(' ') + 
      ' ' + this.chars.vertical;
    
    // Header separator
    const separator = this.chars.teeLeft + this.chars.horizontal.repeat(totalWidth + 2) + this.chars.teeRight;
    
    // Data rows
    const dataRows = rows.map(row => 
      this.chars.vertical + ' ' + 
      row.map((cell, i) => (cell || '').padEnd(columnWidths[i])).join(' ') + 
      ' ' + this.chars.vertical
    );
    
    // Bottom border
    const bottom = this.chars.bottomLeft + this.chars.horizontal.repeat(totalWidth + 2) + this.chars.bottomRight;
    
    return [top, headerRow, separator, ...dataRows, bottom];
  }

  /**
   * Validate box alignment
   */
  validateBox(boxLines) {
    const issues = [];
    
    if (boxLines.length < 3) {
      issues.push('Box must have at least 3 lines (top, content, bottom)');
      return issues;
    }
    
    const expectedWidth = boxLines[0].length;
    
    boxLines.forEach((line, index) => {
      if (line.length !== expectedWidth) {
        issues.push(`Line ${index + 1}: Expected width ${expectedWidth}, got ${line.length}`);
      }
      
      // Check that border lines start and end with proper characters
      if (index === 0) {
        if (!line.startsWith(this.chars.topLeft) || !line.endsWith(this.chars.topRight)) {
          issues.push(`Line ${index + 1}: Invalid top border`);
        }
      } else if (index === boxLines.length - 1) {
        if (!line.startsWith(this.chars.bottomLeft) || !line.endsWith(this.chars.bottomRight)) {
          issues.push(`Line ${index + 1}: Invalid bottom border`);
        }
      } else {
        if (!line.startsWith(this.chars.vertical) || !line.endsWith(this.chars.vertical)) {
          issues.push(`Line ${index + 1}: Invalid side borders`);
        }
      }
    });
    
    return issues;
  }

  /**
   * Generate command bus architecture diagram
   */
  generateCommandBusArchitecture() {
    const outer = this.generateSimpleBox([
      '           Continuum Server              ',
      '         (Orchestrator)                  '
    ], 2);
    
    const commandBus = this.generateSimpleBox([
      '         Command Bus                 '
    ], 2);
    
    const commands = [
      this.generateSimpleBox(['Academy'], 1),
      this.generateSimpleBox(['Screenshot'], 1), 
      this.generateSimpleBox(['Chat'], 1),
      this.generateSimpleBox(['Help'], 1)
    ];
    
    // Combine command boxes horizontally
    const commandRow = [];
    const maxHeight = Math.max(...commands.map(cmd => cmd.length));
    
    for (let i = 0; i < maxHeight; i++) {
      let row = '  ';
      commands.forEach((cmd, cmdIndex) => {
        const line = cmd[i] || ' '.repeat(cmd[0].length);
        row += line;
        if (cmdIndex < commands.length - 1) row += ' ';
      });
      row += '  ';
      commandRow.push(row);
    }
    
    // Build complete diagram
    const result = [];
    
    // Top of outer box
    result.push(outer[0]);
    result.push(outer[1]);
    result.push(outer[2]);
    
    // Command bus section
    result.push('│  ' + commandBus[0] + ' │');
    result.push('│  ' + commandBus[1] + ' │');
    commandRow.forEach(row => result.push('│' + row + '│'));
    result.push('│  ' + commandBus[2] + ' │');
    
    // Bottom of outer box
    result.push(outer[3]);
    
    // Add arrows and client boxes
    result.push('         ↑                    ↑');
    
    const aiPortal = this.generateSimpleBox(['   AI    ', ' Portal  ', '(Python) '], 1);
    const browserUI = this.generateSimpleBox([' Browser ', '   UI    ', '(WebApp) '], 1);
    
    const maxClientHeight = Math.max(aiPortal.length, browserUI.length);
    for (let i = 0; i < maxClientHeight; i++) {
      const leftBox = aiPortal[i] || ' '.repeat(aiPortal[0].length);
      const rightBox = browserUI[i] || ' '.repeat(browserUI[0].length);
      result.push(`   ${leftBox}          ${rightBox}`);
    }
    
    return result;
  }

  /**
   * Export box as code block
   */
  exportAsCodeBlock(boxLines, language = '') {
    return ['```' + language, ...boxLines, '```'];
  }
}

// CLI usage
if (require.main === module) {
  const generator = new ASCIIBoxGenerator();
  
  const command = process.argv[2];
  
  switch (command) {
    case 'simple':
      const text = process.argv[3] || 'Sample Text';
      const box = generator.generateSimpleBox([text]);
      console.log(box.join('\n'));
      break;
      
    case 'command-bus':
      const diagram = generator.generateCommandBusArchitecture();
      console.log(generator.exportAsCodeBlock(diagram).join('\n'));
      break;
      
    case 'validate':
      console.log('ASCII Box Generator Tool');
      console.log('Usage: node ascii-box-generator.js [command] [options]');
      console.log('Commands:');
      console.log('  simple [text]     - Generate simple box');
      console.log('  command-bus       - Generate command bus diagram');
      console.log('  validate         - Show this help');
      break;
      
    default:
      console.log('Use "validate" command to see usage options');
  }
}

module.exports = ASCIIBoxGenerator;