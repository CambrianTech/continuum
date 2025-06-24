/**
 * VerificationArtifact Unit Tests
 * ==============================
 * Tests the git verification artifact specialization of BaseArtifact.
 * Validates inheritance patterns, git context handling, and verification workflows.
 */

const VerificationArtifact = require('../../../../src/core/artifacts/VerificationArtifact.cjs');
const BaseArtifact = require('../../../../src/core/artifacts/BaseArtifact.cjs');
const fs = require('fs');
const path = require('path');

// Test utilities
const TEST_BASE_PATH = '.continuum/test-artifacts';
const TEST_COMMIT_SHA = 'abc123def456';
const TEST_COMMIT_MESSAGE = 'Test commit for verification';

describe('VerificationArtifact', () => {
    let artifact;
    
    beforeEach(() => {
        artifact = new VerificationArtifact(TEST_COMMIT_SHA, TEST_BASE_PATH);
    });
    
    afterEach(async () => {
        // Clean up test artifacts
        try {
            await fs.promises.rm(TEST_BASE_PATH, { recursive: true, force: true });
        } catch (error) {
            // Ignore cleanup errors
        }
    });

    describe('Inheritance and Structure', () => {
        test('should extend BaseArtifact correctly', () => {
            expect(artifact).toBeInstanceOf(BaseArtifact);
            expect(artifact).toBeInstanceOf(VerificationArtifact);
        });

        test('should have verification type', () => {
            expect(artifact.type).toBe('verification');
        });

        test('should generate ID with timestamp and commit SHA', () => {
            expect(artifact.id).toMatch(/^\d{8}_\d{6}_abc123de$/);
        });

        test('should include verification directory in requirements', () => {
            const required = artifact.getRequiredDirectories();
            expect(required).toContain('logs');        // BaseArtifact
            expect(required).toContain('screenshots'); // BaseArtifact  
            expect(required).toContain('verification'); // VerificationArtifact
        });

        test('should create proper path structure', () => {
            const expectedPath = path.join(TEST_BASE_PATH, 'verification', artifact.id);
            expect(artifact.artifactPath).toBe(expectedPath);
        });
    });

    describe('Git Context Management', () => {
        test('should set commit context correctly', () => {
            const changedFiles = ['src/test.js', 'package.json'];
            artifact.setCommitContext(TEST_COMMIT_SHA, TEST_COMMIT_MESSAGE, changedFiles);
            
            expect(artifact.commitSHA).toBe(TEST_COMMIT_SHA);
            expect(artifact.commitMessage).toBe(TEST_COMMIT_MESSAGE);
            expect(artifact.changedFiles).toEqual(changedFiles);
        });

        test('should update metadata with git context', () => {
            const changedFiles = ['file1.js', 'file2.js'];
            artifact.setCommitContext(TEST_COMMIT_SHA, TEST_COMMIT_MESSAGE, changedFiles);
            
            expect(artifact.metadata.commitSHA).toBe(TEST_COMMIT_SHA);
            expect(artifact.metadata.commitMessage).toBe(TEST_COMMIT_MESSAGE);
            expect(artifact.metadata.changedFiles).toBe(2);
            expect(artifact.metadata.gitContext).toBe(true);
        });
    });

    describe('Console Evidence Collection', () => {
        test('should add console evidence entries', () => {
            artifact.addConsoleEvidence({ level: 'log', message: 'Test console log' });
            artifact.addConsoleEvidence({ level: 'error', message: 'Test error' });
            
            expect(artifact.consoleEvidence).toHaveLength(2);
            expect(artifact.consoleEvidence[0]).toMatchObject({
                level: 'log',
                message: 'Test console log',
                source: 'browser_console'
            });
        });

        test('should handle string console evidence', () => {
            artifact.addConsoleEvidence('Simple string message');
            
            expect(artifact.consoleEvidence[0]).toMatchObject({
                level: 'log',
                message: 'Simple string message',
                source: 'browser_console'
            });
        });

        test('should add timestamps to evidence', () => {
            const before = new Date();
            artifact.addConsoleEvidence('Test message');
            const after = new Date();
            
            const timestamp = new Date(artifact.consoleEvidence[0].timestamp);
            expect(timestamp.getTime()).toBeGreaterThanOrEqual(before.getTime());
            expect(timestamp.getTime()).toBeLessThanOrEqual(after.getTime());
        });
    });

    describe('Test Results Management', () => {
        test('should set test results with computed fields', () => {
            const results = {
                tests: ['test1', 'test2', 'test3'],
                passed: 2,
                failed: 1,
                duration: 1500
            };
            
            artifact.setTestResults(results);
            
            expect(artifact.testResults).toMatchObject({
                totalTests: 3,
                passed: 2,
                failed: 1,
                duration: 1500
            });
            expect(artifact.testResults.timestamp).toBeDefined();
        });

        test('should handle minimal test results', () => {
            artifact.setTestResults({ passed: 5 });
            
            expect(artifact.testResults.totalTests).toBe(0);
            expect(artifact.testResults.passed).toBe(5);
            expect(artifact.testResults.failed).toBe(0);
        });
    });

    describe('Verification Status Management', () => {
        test('should set verification status and update metadata', () => {
            artifact.setVerificationStatus('passed', 'All tests successful');
            
            expect(artifact.verificationStatus).toBe('passed');
            expect(artifact.metadata.verificationStatus).toBe('passed');
            expect(artifact.metadata.verificationReason).toBe('All tests successful');
            expect(artifact.metadata.completedAt).toBeDefined();
        });

        test('should handle status without reason', () => {
            artifact.setVerificationStatus('failed');
            
            expect(artifact.verificationStatus).toBe('failed');
            expect(artifact.metadata.verificationReason).toBe('');
        });
    });

    describe('File Generation', () => {
        test('should save verification data to disk', async () => {
            // Set up test data
            artifact.setCommitContext(TEST_COMMIT_SHA, TEST_COMMIT_MESSAGE, ['test.js']);
            artifact.addConsoleEvidence('Test console output');
            artifact.setTestResults({ passed: 3, failed: 0, duration: 1000 });
            artifact.setVerificationStatus('passed');
            
            await artifact.saveVerificationData();
            
            // Verify directory structure
            const verificationDir = path.join(artifact.artifactPath, 'verification');
            expect(fs.existsSync(verificationDir)).toBe(true);
            
            // Verify files exist
            expect(fs.existsSync(path.join(verificationDir, 'commit_info.json'))).toBe(true);
            expect(fs.existsSync(path.join(verificationDir, 'test_results.json'))).toBe(true);
            expect(fs.existsSync(path.join(verificationDir, 'console_evidence.txt'))).toBe(true);
            expect(fs.existsSync(path.join(verificationDir, 'verification_report.txt'))).toBe(true);
            
            // Verify summary.txt exists (BaseArtifact)
            expect(fs.existsSync(path.join(artifact.artifactPath, 'summary.txt'))).toBe(true);
        });

        test('should generate proper verification report', () => {
            artifact.setCommitContext(TEST_COMMIT_SHA, TEST_COMMIT_MESSAGE, ['file1.js']);
            artifact.setTestResults({ passed: 5, failed: 0, totalTests: 5, duration: 2000 });
            artifact.addConsoleEvidence('Console test');
            artifact.setVerificationStatus('passed');
            
            const report = artifact.generateVerificationReport();
            
            expect(report).toContain('✅ PASSED');
            expect(report).toContain(TEST_COMMIT_SHA);
            expect(report).toContain(TEST_COMMIT_MESSAGE);
            expect(report).toContain('Total Tests: 5');
            expect(report).toContain('Passed: 5');
            expect(report).toContain('Failed: 0');
            expect(report).toContain('JTAG Feedback Loop: OPERATIONAL');
        });

        test('should handle failed verification report', () => {
            artifact.setVerificationStatus('failed');
            const report = artifact.generateVerificationReport();
            
            expect(report).toContain('❌ FAILED');
            expect(report).toContain('commit blocked');
        });
    });

    describe('Legacy Compatibility', () => {
        test('should create legacy symlink', async () => {
            await artifact.createDirectories();
            await artifact.createLegacySymlink();
            
            const legacyLink = '.continuum/verification/latest';
            expect(fs.existsSync(legacyLink)).toBe(true);
            
            // Verify it points to our artifact
            const linkTarget = await fs.promises.readlink(legacyLink);
            expect(linkTarget).toContain(artifact.id);
        });
    });

    describe('Integration with BaseArtifact', () => {
        test('should inherit universal logging interface', () => {
            expect(typeof artifact.logClient).toBe('function');
            expect(typeof artifact.logServer).toBe('function');
            expect(typeof artifact.logConsole).toBe('function');
            expect(typeof artifact.logError).toBe('function');
        });

        test('should inherit metadata management', () => {
            artifact.updateMetadata({ customField: 'test' });
            expect(artifact.metadata.customField).toBe('test');
        });

        test('should inherit directory creation', async () => {
            await artifact.createDirectories();
            
            // BaseArtifact directories
            expect(fs.existsSync(path.join(artifact.artifactPath, 'logs'))).toBe(true);
            expect(fs.existsSync(path.join(artifact.artifactPath, 'screenshots'))).toBe(true);
            
            // VerificationArtifact directory
            expect(fs.existsSync(path.join(artifact.artifactPath, 'verification'))).toBe(true);
        });
    });
});