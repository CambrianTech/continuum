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
        description: 'Diagram type: simple, flow, table, command-bus, org-chart',
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
        description: 'Title for flow diagrams and org charts',
        default: null
      },
      levels: {
        type: 'array',
        description: 'Org chart levels: array of arrays (for org-chart type)',
        default: null
      },
      ascii_safe: {
        type: 'boolean',
        description: 'Use ASCII-safe characters for GitHub compatibility',
        default: false
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
        levels = null,
        ascii_safe = false,
        output_format = 'plain'
      } = params;

      // Use Python API for complex diagrams with parameters
      if (type === 'org-chart' || headers || rows || ascii_safe) {
        return await this.executePythonAPI(params);
      }

      // Path to our ASCII generator (in same command package)
      const generatorPath = path.join(__dirname, 'ASCIIBoxGenerator.py');
      
      // Build command arguments for simple CLI usage
      const args = [generatorPath];
      
      if (type === 'command-bus') {
        args.push(ascii_safe ? 'command-bus-safe' : 'command-bus');
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

  async executePythonAPI(params) {
    // For complex parameters, use Python API directly
    const generatorPath = path.join(__dirname, 'ASCIIBoxGenerator.py');
    const scriptCode = `
import sys
sys.path.append('${__dirname}')
from ASCIIBoxGenerator import generate_ascii_diagram
import json

params = ${JSON.stringify(params)}
result = generate_ascii_diagram(
    diagram_type=params.get('type', 'simple'),
    content=params.get('content'),
    ascii_safe=params.get('ascii_safe', False),
    title=params.get('title'),
    levels=params.get('levels'),
    headers=params.get('headers'),
    rows=params.get('rows')
)

for line in result:
    print(line)
`;

    const result = await this.runPythonGenerator(['-c', scriptCode]);
    let output = result.stdout.split('\n').filter(line => line.trim());
    
    if (params.output_format === 'markdown') {
      output = ['```', ...output, '```'];
    }

    return {
      success: true,
      diagram: output,
      type: params.type,
      lines: output.length,
      message: `Generated ${params.type} ASCII diagram with ${output.length} lines`
    };
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
        },
        {
          description: 'Generate AI Academy org chart',
          command: 'ascii_diagram',
          params: {
            type: 'org-chart',
            title: 'Continuum AI Academy',
            levels: [
              ['PlannerAI'],
              ['ArchitectAI', 'UIDesignBot', 'TestingNinja']
            ],
            ascii_safe: true
          }
        }
      ]
    };
  }
}

module.exports = ASCIIDiagramCommand;