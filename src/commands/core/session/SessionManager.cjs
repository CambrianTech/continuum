/**
 * Session Manager - Unified session organization for all Continuum systems
 * 
 * Creates organized session directories for:
 * - verification/ (git hook sessions)
 * - portal/ (AI portal sessions) 
 * - personas/ (persona execution sessions)
 * - sentinels/ (sentinel monitoring sessions)
 * - devtools/ (DevTools automation sessions)
 * 
 * Each session type gets:
 * - Individual run directories: run_HASH/ or run_NAME_TIMESTAMP/
 * - Latest symlink: latest -> most_recent_run/
 * - History tracking: history.txt with session summaries
 * - Consistent artifacts: client-logs.txt, server-logs.txt, ui-capture.png
 */

const fs = require('fs').promises;
const path = require('path');
const { execSync } = require('child_process');

class SessionManager {
    constructor(baseDir = '.') {
        this.baseDir = baseDir;
        this.sessionsDir = path.join(baseDir, 'sessions');
    }

    /**
     * Create a new session directory for a given session type
     * @param {string} sessionType - Type of session (verification, portal, personas, etc.)
     * @param {string} runId - Unique identifier for this run
     * @param {Object} metadata - Session metadata for history tracking
     * @returns {string} Path to created session directory
     */
    async createSession(sessionType, runId, metadata = {}) {
        try {
            // Create sessions directory structure
            const typeDir = path.join(this.sessionsDir, sessionType);
            const runDir = path.join(typeDir, `run_${runId}`);
            
            // Ensure directories exist
            await fs.mkdir(runDir, { recursive: true });
            
            // Create session metadata file
            const sessionMeta = {
                sessionType,
                runId,
                timestamp: new Date().toISOString(),
                startTime: Date.now(),
                ...metadata
            };
            
            await fs.writeFile(
                path.join(runDir, 'session.json'),
                JSON.stringify(sessionMeta, null, 2)
            );
            
            console.log(`📁 Session created: sessions/${sessionType}/run_${runId}/`);
            return runDir;
            
        } catch (error) {
            console.error('❌ Session creation failed:', error);
            throw error;
        }
    }

    /**
     * Complete a session - update latest symlink and history
     * @param {string} sessionType - Type of session
     * @param {string} runId - Run identifier
     * @param {Object} results - Session completion results
     */
    async completeSession(sessionType, runId, results = {}) {
        try {
            const typeDir = path.join(this.sessionsDir, sessionType);
            const runDir = path.join(typeDir, `run_${runId}`);
            const latestLink = path.join(typeDir, 'latest');
            
            // Update session metadata with completion info
            const sessionMetaPath = path.join(runDir, 'session.json');
            let sessionMeta = {};
            
            try {
                const metaContent = await fs.readFile(sessionMetaPath, 'utf8');
                sessionMeta = JSON.parse(metaContent);
            } catch (e) {
                // If no metadata file, create basic one
                sessionMeta = { sessionType, runId, timestamp: new Date().toISOString() };
            }
            
            sessionMeta.endTime = Date.now();
            sessionMeta.duration = sessionMeta.endTime - (sessionMeta.startTime || sessionMeta.endTime);
            sessionMeta.results = results;
            sessionMeta.status = results.success ? 'PASS' : 'FAIL';
            
            await fs.writeFile(sessionMetaPath, JSON.stringify(sessionMeta, null, 2));
            
            // Update latest symlink
            try {
                await fs.unlink(latestLink);
            } catch (e) {
                // Symlink might not exist, that's ok
            }
            
            await fs.symlink(`run_${runId}`, latestLink);
            
            // Update history
            await this.updateHistory(sessionType, sessionMeta);
            
            console.log(`✅ Session completed: sessions/${sessionType}/run_${runId}/`);
            console.log(`🔗 Latest updated: sessions/${sessionType}/latest/`);
            
            return runDir;
            
        } catch (error) {
            console.error('❌ Session completion failed:', error);
            throw error;
        }
    }

    /**
     * Update session history file with unified format
     * @param {string} sessionType - Type of session
     * @param {Object} sessionMeta - Session metadata
     */
    async updateHistory(sessionType, sessionMeta) {
        try {
            const typeDir = path.join(this.sessionsDir, sessionType);
            const historyFile = path.join(typeDir, 'history.txt');
            
            // Unified history format: MM/DD HH:mm STATUS DURATIONs RUN_ID SUMMARY
            const timestamp = new Date(sessionMeta.timestamp).toLocaleString('en-US', {
                month: '2-digit',
                day: '2-digit', 
                hour: '2-digit',
                minute: '2-digit',
                hour12: false
            });
            
            const status = sessionMeta.status === 'PASS' ? '✅' : '❌';
            const duration = sessionMeta.duration ? `${(sessionMeta.duration / 1000).toFixed(1)}s` : 'N/A';
            const runId = sessionMeta.runId.substring(0, 8);
            const summary = sessionMeta.results?.summary || sessionMeta.results?.message || 'Session completed';
            
            // Consistent format across all session types
            const historyEntry = `${timestamp} ${status} ${duration.padStart(6)} ${runId} ${summary.substring(0, 60)}\n`;
            
            // Append to history
            await fs.appendFile(historyFile, historyEntry);
            
        } catch (error) {
            console.error('⚠️ History update failed:', error);
            // Don't throw - history failure shouldn't break session completion
        }
    }

    /**
     * Static factory method - Continuum OS-level service instantiation
     * @param {string} baseDir - Base directory for sessions (default: project root)
     * @returns {SessionManager} Configured session manager instance
     */
    static createForContinuum(baseDir = '.') {
        return new SessionManager(baseDir);
    }

    /**
     * OS-level command interface - trigger session operations via Continuum
     * @param {string} command - Session command (create, complete, read, write, list)
     * @param {Object} params - Command parameters
     * @returns {*} Command result
     */
    async executeSessionCommand(command, params) {
        try {
            switch (command) {
                case 'create':
                    return await this.createSession(params.type, params.runId, params.metadata);
                
                case 'complete':
                    return await this.completeSession(params.type, params.runId, params.results);
                
                case 'read':
                    return await this.readArtifact(params.type, params.runId, params.artifact);
                
                case 'write':
                    return await this.writeToArtifact(params.type, params.runId, params.artifact, params.content, params.append);
                
                case 'list':
                    return await this.listRecentSessions(params.type, params.limit);
                
                case 'path':
                    return await this.getSessionPath(params.type, params.runId);
                
                case 'latest':
                    return await this.getLatestSession(params.type);
                
                default:
                    throw new Error(`Unknown session command: ${command}`);
            }
        } catch (error) {
            console.error(`❌ Session command '${command}' failed:`, error);
            throw error;
        }
    }

    /**
     * Migrate existing verification data to new session structure
     * @param {string} legacyVerificationDir - Path to existing verification/ directory
     */
    async migrateVerificationData(legacyVerificationDir) {
        try {
            console.log('🔄 Migrating verification data to new session structure...');
            
            const fs = require('fs').promises;
            const legacyDirs = await fs.readdir(legacyVerificationDir);
            
            for (const dir of legacyDirs) {
                if (dir.startsWith('verification_')) {
                    const runId = dir.replace('verification_', '');
                    const legacyPath = path.join(legacyVerificationDir, dir);
                    
                    // Create new session
                    const sessionDir = await this.createSession('verification', runId, {
                        migrated: true,
                        originalPath: legacyPath
                    });
                    
                    // Copy artifacts
                    try {
                        await fs.copyFile(
                            path.join(legacyPath, 'client-logs.txt'),
                            path.join(sessionDir, 'client-logs.txt')
                        );
                        await fs.copyFile(
                            path.join(legacyPath, 'server-logs.txt'),
                            path.join(sessionDir, 'server-logs.txt')
                        );
                        await fs.copyFile(
                            path.join(legacyPath, 'ui-capture.png'),
                            path.join(sessionDir, 'ui-capture.png')
                        );
                        
                        console.log(`✅ Migrated: verification_${runId}`);
                    } catch (e) {
                        console.log(`⚠️ Partial migration: verification_${runId} (some files missing)`);
                    }
                }
            }
            
            // Migrate history if it exists
            const legacyHistory = path.join(legacyVerificationDir, 'history.txt');
            try {
                const historyContent = await fs.readFile(legacyHistory, 'utf8');
                const newHistoryPath = path.join(this.sessionsDir, 'verification', 'history.txt');
                await fs.writeFile(newHistoryPath, historyContent);
                console.log('✅ Migrated verification history');
            } catch (e) {
                console.log('⚠️ No legacy history to migrate');
            }
            
            console.log('🎉 Verification data migration complete');
            
        } catch (error) {
            console.error('❌ Migration failed:', error);
            throw error;
        }
    }

    /**
     * Get path to latest session of a given type
     * @param {string} sessionType - Type of session
     * @returns {string|null} Path to latest session or null if none exists
     */
    async getLatestSession(sessionType) {
        try {
            const latestLink = path.join(this.sessionsDir, sessionType, 'latest');
            const stats = await fs.lstat(latestLink);
            
            if (stats.isSymbolicLink()) {
                const target = await fs.readlink(latestLink);
                return path.join(this.sessionsDir, sessionType, target);
            }
            
            return null;
            
        } catch (error) {
            return null; // No latest session exists
        }
    }

    /**
     * Copy artifact file to session directory
     * @param {string} sessionDir - Session directory path
     * @param {string} artifactType - Type of artifact (client-logs, server-logs, ui-capture)
     * @param {string} sourcePath - Source file path
     * @param {string} filename - Optional custom filename
     */
    async addArtifact(sessionDir, artifactType, sourcePath, filename = null) {
        try {
            const artifactName = filename || `${artifactType}.txt`;
            if (artifactType === 'ui-capture' && !filename) {
                artifactName = 'ui-capture.png';
            }
            
            const destPath = path.join(sessionDir, artifactName);
            await fs.copyFile(sourcePath, destPath);
            
            console.log(`📎 Artifact added: ${artifactName}`);
            return destPath;
            
        } catch (error) {
            console.error(`⚠️ Failed to add artifact ${artifactType}:`, error);
            throw error;
        }
    }

    /**
     * Write text content directly as artifact
     * @param {string} sessionDir - Session directory path  
     * @param {string} artifactType - Type of artifact
     * @param {string} content - Text content to write
     */
    async writeArtifact(sessionDir, artifactType, content) {
        try {
            const filename = artifactType.includes('.') ? artifactType : `${artifactType}.txt`;
            const artifactPath = path.join(sessionDir, filename);
            
            await fs.writeFile(artifactPath, content);
            console.log(`📝 Artifact written: ${filename}`);
            return artifactPath;
            
        } catch (error) {
            console.error(`⚠️ Failed to write artifact ${artifactType}:`, error);
            throw error;
        }
    }

    /**
     * List recent sessions for a session type
     * @param {string} sessionType - Type of session
     * @param {number} limit - Number of recent sessions to return
     * @returns {Array} Array of session information
     */
    async listRecentSessions(sessionType, limit = 10) {
        try {
            const typeDir = path.join(this.sessionsDir, sessionType);
            const entries = await fs.readdir(typeDir);
            
            const runDirs = entries
                .filter(entry => entry.startsWith('run_'))
                .sort()
                .slice(-limit);
            
            const sessions = [];
            for (const runDir of runDirs) {
                try {
                    const sessionMetaPath = path.join(typeDir, runDir, 'session.json');
                    const metaContent = await fs.readFile(sessionMetaPath, 'utf8');
                    const sessionMeta = JSON.parse(metaContent);
                    sessions.push(sessionMeta);
                } catch (e) {
                    // Skip sessions without metadata
                }
            }
            
            return sessions;
            
        } catch (error) {
            console.error(`⚠️ Failed to list sessions for ${sessionType}:`, error);
            return [];
        }
    }

    /**
     * Read artifact from any session (easy system access)
     * @param {string} sessionType - Type of session
     * @param {string} runId - Run identifier (or "latest")
     * @param {string} artifactType - Artifact to read (client-logs, server-logs, ui-capture, etc.)
     * @returns {string|Buffer} File content
     */
    async readArtifact(sessionType, runId, artifactType) {
        try {
            let sessionPath;
            
            if (runId === 'latest') {
                sessionPath = await this.getLatestSession(sessionType);
                if (!sessionPath) {
                    throw new Error(`No latest session found for ${sessionType}`);
                }
            } else {
                sessionPath = path.join(this.sessionsDir, sessionType, `run_${runId}`);
            }
            
            const filename = artifactType.includes('.') ? artifactType : 
                           artifactType === 'ui-capture' ? 'ui-capture.png' : `${artifactType}.txt`;
            
            const artifactPath = path.join(sessionPath, filename);
            
            // For images, return buffer; for text, return string
            if (filename.endsWith('.png') || filename.endsWith('.jpg')) {
                return await fs.readFile(artifactPath);
            } else {
                return await fs.readFile(artifactPath, 'utf8');
            }
            
        } catch (error) {
            console.error(`⚠️ Failed to read artifact ${artifactType} from ${sessionType}/${runId}:`, error);
            throw error;
        }
    }

    /**
     * Write or append to artifact in any session (easy system access) 
     * @param {string} sessionType - Type of session
     * @param {string} runId - Run identifier (or "latest")
     * @param {string} artifactType - Artifact to write to
     * @param {string|Buffer} content - Content to write
     * @param {boolean} append - Whether to append (default: false = overwrite)
     */
    async writeToArtifact(sessionType, runId, artifactType, content, append = false) {
        try {
            let sessionPath;
            
            if (runId === 'latest') {
                sessionPath = await this.getLatestSession(sessionType);
                if (!sessionPath) {
                    throw new Error(`No latest session found for ${sessionType}`);
                }
            } else {
                sessionPath = path.join(this.sessionsDir, sessionType, `run_${runId}`);
            }
            
            const filename = artifactType.includes('.') ? artifactType : 
                           artifactType === 'ui-capture' ? 'ui-capture.png' : `${artifactType}.txt`;
            
            const artifactPath = path.join(sessionPath, filename);
            
            if (append) {
                await fs.appendFile(artifactPath, content);
                console.log(`📝 Appended to: ${sessionType}/run_${runId}/${filename}`);
            } else {
                await fs.writeFile(artifactPath, content);
                console.log(`📝 Written to: ${sessionType}/run_${runId}/${filename}`);
            }
            
            return artifactPath;
            
        } catch (error) {
            console.error(`⚠️ Failed to write artifact ${artifactType} to ${sessionType}/${runId}:`, error);
            throw error;
        }
    }

    /**
     * Get session directory path (for direct file system access)
     * @param {string} sessionType - Type of session
     * @param {string} runId - Run identifier (or "latest")
     * @returns {string} Full path to session directory
     */
    async getSessionPath(sessionType, runId) {
        try {
            if (runId === 'latest') {
                return await this.getLatestSession(sessionType);
            } else {
                return path.join(this.sessionsDir, sessionType, `run_${runId}`);
            }
        } catch (error) {
            console.error(`⚠️ Failed to get session path for ${sessionType}/${runId}:`, error);
            throw error;
        }
    }

    /**
     * Easy access methods for common operations
     */
    
    // Read latest portal screenshot
    async getLatestPortalScreenshot() {
        return await this.readArtifact('portal', 'latest', 'ui-capture');
    }
    
    // Read latest verification logs
    async getLatestVerificationLogs() {
        const clientLogs = await this.readArtifact('verification', 'latest', 'client-logs');
        const serverLogs = await this.readArtifact('verification', 'latest', 'server-logs');
        return { clientLogs, serverLogs };
    }
    
    // Add log entry to any session
    async addLogEntry(sessionType, runId, logType, message) {
        const timestamp = new Date().toISOString();
        const logEntry = `[${timestamp}] ${message}\n`;
        return await this.writeToArtifact(sessionType, runId, logType, logEntry, true);
    }
}

module.exports = SessionManager;