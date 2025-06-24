/**
 * BaseArtifact - Foundation class for all Continuum artifacts
 * 
 * Defines the minimal required structure that ALL artifacts must have:
 * - artifact.json (metadata)
 * - summary.txt (human readable)
 * - logs/ (universal logging)
 * - screenshots/ (visual capture capability)
 * 
 * Inheritance drives both code behavior AND directory structure.
 */

const fs = require('fs').promises;
const path = require('path');

class BaseArtifact {
    constructor(type, id, basePath = '.continuum/artifacts') {
        this.type = type;
        this.id = id;
        this.timestamp = new Date();
        this.status = 'CREATED';
        this.artifactPath = this.computePath(basePath);
        
        // Metadata that all artifacts share
        this.metadata = {
            id: this.id,
            type: this.type,
            timestamp: this.timestamp.toISOString(),
            status: this.status,
            summary: '',
            version: '1.0.0'
        };
    }
    
    computePath(basePath) {
        const date = this.timestamp;
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, '0');
        
        return path.join(
            basePath,
            this.type,
            year.toString(),
            month,
            this.id
        );
    }
    
    /**
     * Base directory structure that ALL artifacts get
     * Subclasses should call super.getRequiredDirectories() and extend
     */
    getRequiredDirectories() {
        return [
            'logs',        // Universal: client.log, server.log, console.log, errors.log
            'screenshots'  // Universal: screenshot capture capability
        ];
    }
    
    /**
     * Base files that ALL artifacts get
     */
    getRequiredFiles() {
        return [
            'artifact.json',  // Metadata
            'summary.txt'     // Human readable summary
        ];
    }
    
    /**
     * Create the complete directory structure for this artifact
     */
    async createStructure() {
        // Ensure base artifact directory exists
        await this.ensureDir(this.artifactPath);
        
        // Create all required directories
        const directories = this.getRequiredDirectories();
        for (const dir of directories) {
            await this.ensureDir(path.join(this.artifactPath, dir));
        }
        
        // Create required files
        await this.createRequiredFiles();
        
        // Allow subclasses to add their specific structure
        await this.createExtendedStructure();
    }
    
    /**
     * Override in subclasses to add type-specific structure
     */
    async createExtendedStructure() {
        // Base implementation does nothing
        // Subclasses override to add their directories/files
    }
    
    async createRequiredFiles() {
        // Create artifact.json with metadata
        await this.writeArtifactJson();
        
        // Create empty summary.txt
        await this.writeSummary('Artifact created');
    }
    
    async writeArtifactJson() {
        const jsonPath = path.join(this.artifactPath, 'artifact.json');
        await fs.writeFile(jsonPath, JSON.stringify(this.metadata, null, 2));
    }
    
    async writeSummary(summary) {
        this.metadata.summary = summary;
        const summaryPath = path.join(this.artifactPath, 'summary.txt');
        await fs.writeFile(summaryPath, summary);
        
        // Update metadata too
        await this.writeArtifactJson();
    }
    
    async updateStatus(status) {
        this.status = status;
        this.metadata.status = status;
        await this.writeArtifactJson();
    }
    
    /**
     * Standard logging interface that all artifacts inherit
     */
    async logClient(message) {
        await this.appendLog('client.log', `[${new Date().toISOString()}] ${message}`);
    }
    
    async logServer(message) {
        await this.appendLog('server.log', `[${new Date().toISOString()}] ${message}`);
    }
    
    async logConsole(message) {
        await this.appendLog('console.log', `[${new Date().toISOString()}] ${message}`);
    }
    
    async logCommand(command, result) {
        await this.appendLog('commands.log', `[${new Date().toISOString()}] ${command} -> ${result}`);
    }
    
    async logError(error, source = 'unknown') {
        const errorMsg = `[${new Date().toISOString()}] ERROR: ${error}`;
        await this.appendLog('errors.log', errorMsg);
        
        // Also log to specific source log
        if (source === 'client') await this.logClient(`ERROR: ${error}`);
        if (source === 'server') await this.logServer(`ERROR: ${error}`);
    }
    
    async appendLog(filename, content) {
        const logPath = path.join(this.artifactPath, 'logs', filename);
        await fs.appendFile(logPath, content + '\n');
    }
    
    /**
     * Screenshot management that all artifacts inherit
     */
    async addScreenshot(filename, sourcePath) {
        const screenshotPath = path.join(this.artifactPath, 'screenshots', filename);
        
        if (sourcePath) {
            // Copy from source
            await fs.copyFile(sourcePath, screenshotPath);
        }
        
        return screenshotPath;
    }
    
    /**
     * Utility methods
     */
    async ensureDir(dirPath) {
        await fs.mkdir(dirPath, { recursive: true });
    }
    
    async exists(filePath) {
        try {
            await fs.access(filePath);
            return true;
        } catch {
            return false;
        }
    }
    
    /**
     * Self-description for widgets
     */
    getStructureManifest() {
        return {
            artifactPath: this.artifactPath,
            type: this.type,
            directories: this.getRequiredDirectories().map(dir => ({
                name: dir,
                path: path.join(this.artifactPath, dir),
                purpose: this.getDirectoryPurpose(dir)
            })),
            files: this.getRequiredFiles().map(file => ({
                name: file,
                path: path.join(this.artifactPath, file),
                purpose: this.getFilePurpose(file)
            }))
        };
    }
    
    getDirectoryPurpose(dir) {
        const purposes = {
            'logs': 'System and application log files',
            'screenshots': 'Visual captures of system state'
        };
        return purposes[dir] || 'Additional artifact data';
    }
    
    getFilePurpose(file) {
        const purposes = {
            'artifact.json': 'Artifact metadata and status',
            'summary.txt': 'Human-readable artifact summary'
        };
        return purposes[file] || 'Artifact data file';
    }
}

module.exports = BaseArtifact;