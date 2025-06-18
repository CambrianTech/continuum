/**
 * Workspace Command - Directory and workspace management
 * Replaces hardcoded path logic with configurable workspace paths
 */

const BaseCommand = require('../../BaseCommand.cjs');
const fs = require('fs');
const path = require('path');

class WorkspaceCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'workspace',
      description: 'Manage workspace directories and paths',
      icon: '📁',
      parameters: {
        action: {
          type: 'string',
          required: false,
          description: 'Action: path, create, list, info',
          default: 'path'
        },
        workspace: {
          type: 'string',
          required: false,
          description: 'Workspace name (ai-portal, sentinel, screenshots, etc)',
          default: 'ai-portal'
        },
        subdir: {
          type: 'string',
          required: false,
          description: 'Subdirectory within workspace',
          default: ''
        }
      },
      examples: [
        'workspace',
        'workspace --action path --workspace ai-portal',
        'workspace --action path --workspace sentinel --subdir debug',
        'workspace --action create --workspace my-project',
        'workspace --action list',
        'workspace --action info'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    const action = options.action || 'path';
    const workspace = options.workspace || 'ai-portal';
    const subdir = options.subdir || '';
    
    try {
      // Get continuum directory
      const continuumDir = continuum?.localContinuumDir || continuum?.userDataDir || path.join(process.cwd(), '.continuum');
      
      if (action === 'path') {
        return this.getWorkspacePath(continuumDir, workspace, subdir);
      } else if (action === 'create') {
        return this.createWorkspace(continuumDir, workspace, subdir);
      } else if (action === 'list') {
        return this.listWorkspaces(continuumDir);
      } else if (action === 'info') {
        return this.getWorkspaceInfo(continuumDir);
      } else {
        return this.createErrorResult('Invalid action', `Unknown action: ${action}`);
      }
      
    } catch (error) {
      return this.createErrorResult('Workspace command failed', error.message);
    }
  }
  
  static getWorkspacePath(continuumDir, workspace, subdir) {
    const workspacePath = path.join(continuumDir, workspace);
    const finalPath = subdir ? path.join(workspacePath, subdir) : workspacePath;
    
    // Ensure directory exists
    if (!fs.existsSync(finalPath)) {
      fs.mkdirSync(finalPath, { recursive: true });
    }
    
    return this.createSuccessResult({
      continuumDir,
      workspace,
      subdir,
      path: finalPath,
      exists: fs.existsSync(finalPath)
    }, `Workspace path: ${finalPath}`);
  }
  
  static createWorkspace(continuumDir, workspace, subdir) {
    const workspacePath = path.join(continuumDir, workspace);
    const finalPath = subdir ? path.join(workspacePath, subdir) : workspacePath;
    
    try {
      fs.mkdirSync(finalPath, { recursive: true });
      
      // Create a simple README
      const readmePath = path.join(finalPath, 'README.md');
      if (!fs.existsSync(readmePath)) {
        const readme = `# ${workspace} Workspace\n\nCreated: ${new Date().toISOString()}\nPath: ${finalPath}\n`;
        fs.writeFileSync(readmePath, readme);
      }
      
      return this.createSuccessResult({
        workspace,
        path: finalPath,
        created: true
      }, `Created workspace: ${finalPath}`);
      
    } catch (error) {
      return this.createErrorResult('Failed to create workspace', error.message);
    }
  }
  
  static listWorkspaces(continuumDir) {
    if (!fs.existsSync(continuumDir)) {
      return this.createSuccessResult({ workspaces: [] }, 'No continuum directory found');
    }
    
    const items = fs.readdirSync(continuumDir, { withFileTypes: true });
    const workspaces = items
      .filter(item => item.isDirectory())
      .map(item => {
        const workspacePath = path.join(continuumDir, item.name);
        const stats = fs.statSync(workspacePath);
        
        // Count files/subdirs
        let fileCount = 0;
        let dirCount = 0;
        try {
          const contents = fs.readdirSync(workspacePath, { withFileTypes: true });
          fileCount = contents.filter(c => c.isFile()).length;
          dirCount = contents.filter(c => c.isDirectory()).length;
        } catch (e) {
          // Permission error or similar
        }
        
        return {
          name: item.name,
          path: workspacePath,
          created: stats.birthtime.toISOString(),
          modified: stats.mtime.toISOString(),
          files: fileCount,
          directories: dirCount
        };
      });
    
    return this.createSuccessResult({ 
      continuumDir,
      workspaces,
      count: workspaces.length
    }, `Found ${workspaces.length} workspaces`);
  }
  
  static getWorkspaceInfo(continuumDir) {
    const info = {
      continuumDir,
      exists: fs.existsSync(continuumDir),
      common_workspaces: [
        'ai-portal',
        'sentinel', 
        'screenshots',
        'logs',
        'scripts',
        'cache',
        'temp'
      ]
    };
    
    if (info.exists) {
      try {
        const stats = fs.statSync(continuumDir);
        info.created = stats.birthtime.toISOString();
        info.modified = stats.mtime.toISOString();
        
        // Get total size
        info.size = this.getDirectorySize(continuumDir);
      } catch (e) {
        info.error = e.message;
      }
    }
    
    return this.createSuccessResult(info, `Workspace info for: ${continuumDir}`);
  }
  
  static getDirectorySize(dirPath) {
    let totalSize = 0;
    
    try {
      const items = fs.readdirSync(dirPath, { withFileTypes: true });
      
      for (const item of items) {
        const itemPath = path.join(dirPath, item.name);
        
        if (item.isFile()) {
          const stats = fs.statSync(itemPath);
          totalSize += stats.size;
        } else if (item.isDirectory()) {
          totalSize += this.getDirectorySize(itemPath);
        }
      }
    } catch (e) {
      // Permission error or similar
    }
    
    return totalSize;
  }
}

module.exports = WorkspaceCommand;