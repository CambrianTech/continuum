/**
 * ASCII Diagram Command
 * 
 * Generates properly aligned ASCII art diagrams for documentation
 */

const { spawn } = require('child_process');
const path = require('path');

class ASCIIDiagramCommand {
  constructor() {
    this.name = 'ascii_diagram';
    this.description = 'Generate ASCII art diagrams for documentation';
    
    this.parameters = {
      type: {
        type: 'string',
        description: 'Diagram type: simple, flow, table, command-bus',
        default: 'simple'
      },
      content: {
        type: 'array',
        description: 'Content lines for the diagram',
        default: ['Sample Text']
      },
      headers: {
        type: 'array', 
        description: 'Table headers (for table type)',
        default: null
      },
      rows: {
        type: 'array',
        description: 'Table rows (for table type)', 
        default: null
      },
      title: {
        type: 'string',
        description: 'Title for flow diagrams',
        default: null
      },
      output_format: {
        type: 'string',
        description: 'Output format: plain, markdown',
        default: 'plain'
      }
    };
  }

  async execute(params = {}) {
    try {
      const {
        type = 'simple',
        content = ['Sample Text'],
        headers = null,
        rows = null, 
        title = null,
        output_format = 'plain'
      } = params;

      // Path to our ASCII generator
      const generatorPath = path.join(__dirname, '../../../core/ASCIIBoxGenerator.py');
      
      // Build command arguments
      const args = [generatorPath];
      
      if (type === 'command-bus') {
        args.push('command-bus');
      } else if (type === 'simple' && content.length > 0) {
        args.push('simple', content[0]);
      }

      // Execute the Python generator
      const result = await this.runPythonGenerator(args);
      
      // Format output
      let output = result.stdout.split('\n').filter(line => line.trim());
      
      if (output_format === 'markdown') {
        output = ['```', ...output, '```'];
      }

      return {
        success: true,
        diagram: output,
        type: type,
        lines: output.length,
        message: `Generated ${type} ASCII diagram with ${output.length} lines`
      };

    } catch (error) {
      return {
        success: false,
        error: error.message,
        message: 'Failed to generate ASCII diagram'
      };
    }
  }

  async runPythonGenerator(args) {
    return new Promise((resolve, reject) => {
      const process = spawn('python3', args);
      
      let stdout = '';
      let stderr = '';
      
      process.stdout.on('data', (data) => {
        stdout += data.toString();
      });
      
      process.stderr.on('data', (data) => {
        stderr += data.toString();
      });
      
      process.on('close', (code) => {
        if (code === 0) {
          resolve({ stdout, stderr });
        } else {
          reject(new Error(`Python process exited with code ${code}: ${stderr}`));
        }
      });
      
      process.on('error', (error) => {
        reject(error);
      });
    });
  }

  getHelp() {
    return {
      name: this.name,
      description: this.description,
      parameters: this.parameters,
      examples: [
        {
          description: 'Generate simple text box',
          command: 'ascii_diagram',
          params: {
            type: 'simple',
            content: ['Hello World']
          }
        },
        {
          description: 'Generate command bus architecture',
          command: 'ascii_diagram', 
          params: {
            type: 'command-bus',
            output_format: 'markdown'
          }
        },
        {
          description: 'Generate flow diagram',
          command: 'ascii_diagram',
          params: {
            type: 'flow',
            title: 'Process Flow',
            content: ['Step 1: Initialize', 'Step 2: Process', 'Step 3: Complete']
          }
        }
      ]
    };
  }
}

module.exports = ASCIIDiagramCommand;