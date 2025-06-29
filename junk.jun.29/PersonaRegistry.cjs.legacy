/**
 * PersonaRegistry - Hierarchical persona storage and discovery
 * 
 * Storage Hierarchy (in priority order):
 * 1. Project: $repo/.continuum/personas/ (highest priority)
 * 2. User: $HOME/.continuum/personas/ (personal personas)
 * 3. Organization: $ORG_SHARE/.continuum/personas/ (shared/team personas)
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

class PersonaRegistry {
  constructor(options = {}) {
    this.orgSharePath = options.orgSharePath || process.env.CONTINUUM_ORG_SHARE;
    this.searchPaths = this.buildSearchPaths();
  }

  /**
   * Build hierarchical search paths in priority order
   */
  buildSearchPaths() {
    const paths = [];
    
    // 1. Project-specific (.continuum in current repo)
    const projectPath = path.join(process.cwd(), '.continuum', 'personas');
    paths.push({ type: 'project', path: projectPath, priority: 1 });
    
    // 2. User-specific ($HOME/.continuum)
    const userPath = path.join(os.homedir(), '.continuum', 'personas');
    paths.push({ type: 'user', path: userPath, priority: 2 });
    
    // 3. Organization share (if configured)
    if (this.orgSharePath) {
      const orgPath = path.join(this.orgSharePath, '.continuum', 'personas');
      paths.push({ type: 'organization', path: orgPath, priority: 3 });
    }
    
    return paths;
  }

  /**
   * Get the appropriate storage location for saving a persona
   */
  getStorageLocation(scope = 'user') {
    switch (scope) {
      case 'project':
        return this.searchPaths.find(p => p.type === 'project')?.path;
      case 'user':
        return this.searchPaths.find(p => p.type === 'user')?.path;
      case 'organization':
        return this.searchPaths.find(p => p.type === 'organization')?.path;
      default:
        // Default to user scope
        return this.searchPaths.find(p => p.type === 'user')?.path;
    }
  }

  /**
   * Ensure storage directory exists
   */
  ensureStorageDir(scope = 'user') {
    const storagePath = this.getStorageLocation(scope);
    if (!storagePath) {
      throw new Error(`Storage location not configured for scope: ${scope}`);
    }
    
    if (!fs.existsSync(storagePath)) {
      fs.mkdirSync(storagePath, { recursive: true });
      console.log(`📁 Created persona storage: ${storagePath}`);
    }
    
    return storagePath;
  }

  /**
   * Find a persona by ID across all search paths
   */
  findPersona(personaId) {
    for (const searchLocation of this.searchPaths) {
      const personaDir = path.join(searchLocation.path, personaId);
      const configPath = path.join(personaDir, 'config.json');
      
      if (fs.existsSync(configPath)) {
        return {
          found: true,
          location: searchLocation,
          personaDir,
          configPath
        };
      }
    }
    
    return { found: false };
  }

  /**
   * List all personas across all search paths
   */
  listAllPersonas() {
    const allPersonas = [];
    const seenIds = new Set(); // Avoid duplicates (project overrides user overrides org)
    
    for (const searchLocation of this.searchPaths) {
      if (!fs.existsSync(searchLocation.path)) {
        continue;
      }
      
      const entries = fs.readdirSync(searchLocation.path);
      
      for (const entry of entries) {
        // Skip if we've already seen this persona ID (higher priority location wins)
        if (seenIds.has(entry)) {
          continue;
        }
        
        const configPath = path.join(searchLocation.path, entry, 'config.json');
        if (fs.existsSync(configPath)) {
          try {
            const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
            allPersonas.push({
              id: entry,
              name: config.metadata?.name || entry,
              specialization: config.metadata?.specialty || 'general',
              status: config.status || 'unknown',
              scope: searchLocation.type,
              location: searchLocation.path,
              priority: searchLocation.priority
            });
            seenIds.add(entry);
          } catch (error) {
            console.warn(`⚠️ Failed to read persona config: ${configPath}`);
          }
        }
      }
    }
    
    return allPersonas.sort((a, b) => a.priority - b.priority);
  }

  /**
   * Share a persona from user scope to organization scope
   */
  async sharePersona(personaId, fromScope = 'user', toScope = 'organization') {
    const sourceLocation = this.getStorageLocation(fromScope);
    const targetLocation = this.getStorageLocation(toScope);
    
    if (!sourceLocation || !targetLocation) {
      throw new Error(`Invalid scope combination: ${fromScope} -> ${toScope}`);
    }
    
    const sourceDir = path.join(sourceLocation, personaId);
    const targetDir = path.join(targetLocation, personaId);
    
    if (!fs.existsSync(sourceDir)) {
      throw new Error(`Persona not found in ${fromScope}: ${personaId}`);
    }
    
    // Ensure target directory exists
    this.ensureStorageDir(toScope);
    
    // Copy persona directory
    await this.copyDirectory(sourceDir, targetDir);
    
    console.log(`🤝 Shared persona: ${personaId}`);
    console.log(`   From: ${fromScope} (${sourceDir})`);
    console.log(`   To: ${toScope} (${targetDir})`);
    
    return {
      personaId,
      fromScope,
      toScope,
      sourcePath: sourceDir,
      targetPath: targetDir
    };
  }

  /**
   * Get persona storage statistics
   */
  getStorageStats() {
    const stats = {
      searchPaths: this.searchPaths.map(sp => ({
        type: sp.type,
        path: sp.path,
        exists: fs.existsSync(sp.path),
        priority: sp.priority
      })),
      personaCounts: {}
    };
    
    for (const searchLocation of this.searchPaths) {
      if (fs.existsSync(searchLocation.path)) {
        const entries = fs.readdirSync(searchLocation.path);
        const validPersonas = entries.filter(entry => {
          const configPath = path.join(searchLocation.path, entry, 'config.json');
          return fs.existsSync(configPath);
        });
        stats.personaCounts[searchLocation.type] = validPersonas.length;
      } else {
        stats.personaCounts[searchLocation.type] = 0;
      }
    }
    
    return stats;
  }

  /**
   * Utility: Copy directory recursively
   */
  async copyDirectory(source, target) {
    if (!fs.existsSync(target)) {
      fs.mkdirSync(target, { recursive: true });
    }
    
    const entries = fs.readdirSync(source, { withFileTypes: true });
    
    for (const entry of entries) {
      const sourcePath = path.join(source, entry.name);
      const targetPath = path.join(target, entry.name);
      
      if (entry.isDirectory()) {
        await this.copyDirectory(sourcePath, targetPath);
      } else {
        fs.copyFileSync(sourcePath, targetPath);
      }
    }
  }

  /**
   * Check if organization sharing is configured
   */
  isOrgSharingEnabled() {
    return !!this.orgSharePath && fs.existsSync(this.orgSharePath);
  }

  /**
   * Get configuration info
   */
  getConfig() {
    return {
      orgSharePath: this.orgSharePath,
      orgSharingEnabled: this.isOrgSharingEnabled(),
      searchPaths: this.searchPaths.map(sp => ({
        type: sp.type,
        path: sp.path,
        exists: fs.existsSync(sp.path)
      }))
    };
  }
}

module.exports = PersonaRegistry;