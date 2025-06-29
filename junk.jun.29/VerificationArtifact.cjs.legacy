/**
 * VerificationArtifact - Git Hook Verification Specialized Artifact
 * ================================================================
 * Extends BaseArtifact to provide git commit verification specific functionality.
 * 
 * DIRECTORY STRUCTURE (inheritance-driven):
 * .continuum/artifacts/verification/YYYYMMDD_HHMMSS_SHA/
 * ├── artifact.json           # BaseArtifact metadata
 * ├── summary.txt             # BaseArtifact summary  
 * ├── logs/                   # BaseArtifact universal logging
 * │   ├── client.log          # Portal/DevTools client output
 * │   ├── server.log          # Continuum server output
 * │   ├── console.log         # Browser console forwarded logs
 * │   └── errors.log          # Error capture and stack traces
 * ├── screenshots/            # BaseArtifact visual proof
 * │   └── ui-capture.png      # Verification UI state screenshot
 * └── verification/           # VerificationArtifact specific
 *     ├── commit_info.json    # Git commit metadata (SHA, message, files)
 *     ├── test_results.json   # Test execution results and timings
 *     ├── console_evidence.txt # Browser console test evidence 
 *     └── verification_report.txt # Human-readable verification summary
 * 
 * FEATURES:
 * - Git commit context preservation
 * - Browser console evidence collection
 * - Test execution result tracking
 * - JTAG feedback loop validation
 * - Emergency verification system integration
 */

const BaseArtifact = require('./BaseArtifact.cjs');
const fs = require('fs');
const path = require('path');

class VerificationArtifact extends BaseArtifact {
    constructor(commitSHA, basePath = '.continuum/artifacts') {
        const timestamp = new Date().toISOString().replace(/[:.-]/g, '').replace('T', '_').slice(0, 15);
        const id = `${timestamp}_${commitSHA.slice(0, 8)}`;
        super('verification', id, basePath);
        
        this.commitSHA = commitSHA;
        this.commitMessage = '';
        this.changedFiles = [];
        this.testResults = {};
        this.consoleEvidence = [];
        this.verificationStatus = 'pending'; // pending, passed, failed
    }

    /**
     * VerificationArtifact-specific directory requirements
     * Adds verification/ subdirectory to BaseArtifact structure
     */
    getRequiredDirectories() {
        return [
            ...super.getRequiredDirectories(), // logs/, screenshots/
            'verification'  // Git verification specific data
        ];
    }

    /**
     * Set git commit context from git commands
     */
    setCommitContext(commitSHA, commitMessage, changedFiles = []) {
        this.commitSHA = commitSHA;
        this.commitMessage = commitMessage;
        this.changedFiles = changedFiles;
        
        // Update artifact.json with commit context
        this.metadata.commitSHA = commitSHA;
        this.metadata.commitMessage = commitMessage;
        this.metadata.changedFiles = changedFiles.length;
        this.metadata.gitContext = true;
    }

    /**
     * Add browser console evidence from DevTools Protocol
     */
    addConsoleEvidence(logEntry) {
        this.consoleEvidence.push({
            timestamp: new Date().toISOString(),
            level: logEntry.level || 'log',
            message: logEntry.message || logEntry.text || logEntry,
            source: 'browser_console'
        });
    }

    /**
     * Set test execution results
     */
    setTestResults(results) {
        this.testResults = {
            ...results,
            timestamp: new Date().toISOString(),
            totalTests: results.tests?.length || 0,
            passed: results.passed || 0,
            failed: results.failed || 0,
            duration: results.duration || 0
        };
    }

    /**
     * Set final verification status with reason
     */
    setVerificationStatus(status, reason = '') {
        this.verificationStatus = status;
        this.metadata.verificationStatus = status;
        this.metadata.verificationReason = reason;
        this.metadata.completedAt = new Date().toISOString();
    }

    /**
     * Save all verification-specific data to disk
     * Creates verification/ subdirectory with specialized files
     */
    async saveVerificationData() {
        await this.createStructure();
        
        const verificationDir = path.join(this.artifactPath, 'verification');
        
        // Git commit information
        const commitInfo = {
            sha: this.commitSHA,
            message: this.commitMessage,
            changedFiles: this.changedFiles,
            timestamp: this.timestamp.toISOString(),
            author: process.env.GIT_AUTHOR_NAME || 'unknown'
        };
        await fs.promises.writeFile(
            path.join(verificationDir, 'commit_info.json'),
            JSON.stringify(commitInfo, null, 2)
        );

        // Test results
        await fs.promises.writeFile(
            path.join(verificationDir, 'test_results.json'),
            JSON.stringify(this.testResults, null, 2)
        );

        // Console evidence for debugging
        const consoleText = this.consoleEvidence
            .map(entry => `[${entry.timestamp}] ${entry.level.toUpperCase()}: ${entry.message}`)
            .join('\n');
        await fs.promises.writeFile(
            path.join(verificationDir, 'console_evidence.txt'),
            `# Browser Console Evidence - Git Verification\n\n${consoleText}\n`
        );

        // Human-readable verification report
        const report = this.generateVerificationReport();
        await fs.promises.writeFile(
            path.join(verificationDir, 'verification_report.txt'),
            report
        );

        // Update summary.txt with verification-specific information
        const summary = `Git Verification - ${this.commitSHA.slice(0, 8)}\n\n` +
            `Status: ${this.verificationStatus.toUpperCase()}\n` +
            `Commit: ${this.commitMessage}\n` +
            `Files Changed: ${this.changedFiles.length}\n` +
            `Test Results: ${this.testResults.passed}/${this.testResults.totalTests} passed\n` +
            `Console Evidence: ${this.consoleEvidence.length} entries\n` +
            `Duration: ${this.testResults.duration}ms\n\n` +
            `This verification confirms git commit safety and JTAG feedback capability.`;
            
        await this.writeSummary(summary);
    }

    /**
     * Generate human-readable verification report
     */
    generateVerificationReport() {
        const status = this.verificationStatus === 'passed' ? '✅ PASSED' : 
                      this.verificationStatus === 'failed' ? '❌ FAILED' : '🟡 PENDING';
        
        return `CONTINUUM GIT VERIFICATION REPORT\n` +
               `=====================================\n\n` +
               `Verification ID: ${this.id}\n` +
               `Commit SHA: ${this.commitSHA}\n` +
               `Status: ${status}\n` +
               `Timestamp: ${this.timestamp.toISOString()}\n\n` +
               `COMMIT DETAILS:\n` +
               `Message: ${this.commitMessage}\n` +
               `Changed Files: ${this.changedFiles.length}\n` +
               `Files: ${this.changedFiles.join(', ')}\n\n` +
               `TEST EXECUTION:\n` +
               `Total Tests: ${this.testResults.totalTests || 0}\n` +
               `Passed: ${this.testResults.passed || 0}\n` +
               `Failed: ${this.testResults.failed || 0}\n` +
               `Duration: ${this.testResults.duration || 0}ms\n\n` +
               `CONSOLE EVIDENCE:\n` +
               `Browser Console Entries: ${this.consoleEvidence.length}\n` +
               `JTAG Feedback Loop: ${this.consoleEvidence.length > 0 ? 'OPERATIONAL' : 'NO EVIDENCE'}\n\n` +
               `VERIFICATION SUMMARY:\n` +
               `${this.verificationStatus === 'passed' ? 
                  '✅ All systems operational - commit approved' :
                  this.verificationStatus === 'failed' ?
                  '❌ Verification failed - commit blocked' :
                  '🟡 Verification in progress'}\n`;
    }

    /**
     * Create latest symlink in artifact directory structure
     * Points .continuum/artifacts/verification/latest -> current verification artifact
     */
    async createLatestSymlink() {
        const artifactTypeDir = path.join('.continuum/artifacts', this.type);
        const latestLink = path.join(artifactTypeDir, 'latest');
        
        // Ensure artifact type directory exists
        await fs.promises.mkdir(artifactTypeDir, { recursive: true });
        
        // Remove existing symlink if present
        try {
            await fs.promises.unlink(latestLink);
        } catch (error) {
            // Ignore if symlink doesn't exist
        }
        
        // Create new symlink pointing to our artifact
        const relativePath = path.relative(artifactTypeDir, this.artifactPath);
        await fs.promises.symlink(relativePath, latestLink);
    }

    /**
     * Create legacy symlink for backward compatibility
     * Points verification/latest -> current verification artifact
     */
    async createLegacySymlink() {
        const legacyDir = '.continuum/verification';
        const latestLink = path.join(legacyDir, 'latest');
        
        // Ensure legacy directory exists
        await fs.promises.mkdir(legacyDir, { recursive: true });
        
        // Remove existing symlink if present
        try {
            await fs.promises.unlink(latestLink);
        } catch (error) {
            // Ignore if symlink doesn't exist
        }
        
        // Create new symlink pointing to our artifact
        const relativePath = path.relative(legacyDir, this.artifactPath);
        await fs.promises.symlink(relativePath, latestLink);
    }
}

module.exports = VerificationArtifact;