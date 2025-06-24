/**
 * BaseArtifact Unit Tests
 * 
 * Tests the foundation artifact class and establishes testing patterns
 * that subclass tests will inherit and extend.
 */

const fs = require('fs').promises;
const path = require('path');
const os = require('os');
const BaseArtifact = require('../../../../src/core/artifacts/BaseArtifact.cjs');

describe('BaseArtifact', () => {
    let tempDir;
    let artifact;
    
    beforeEach(async () => {
        // Create temporary directory for each test
        tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'continuum-test-'));
        artifact = new BaseArtifact('test_type', 'test_id', tempDir);
    });
    
    afterEach(async () => {
        // Clean up temporary directory
        await fs.rm(tempDir, { recursive: true, force: true });
    });
    
    describe('Construction', () => {
        test('creates artifact with correct metadata', () => {
            expect(artifact.type).toBe('test_type');
            expect(artifact.id).toBe('test_id');
            expect(artifact.status).toBe('CREATED');
            expect(artifact.metadata.type).toBe('test_type');
            expect(artifact.metadata.id).toBe('test_id');
        });
        
        test('computes hierarchical path correctly', () => {
            const date = artifact.timestamp;
            const year = date.getFullYear();
            const month = String(date.getMonth() + 1).padStart(2, '0');
            
            const expectedPath = path.join(
                tempDir,
                'test_type',
                year.toString(),
                month,
                'test_id'
            );
            
            expect(artifact.artifactPath).toBe(expectedPath);
        });
    });
    
    describe('Directory Structure', () => {
        test('defines required base directories', () => {
            const directories = artifact.getRequiredDirectories();
            expect(directories).toContain('logs');
            expect(directories).toContain('screenshots');
        });
        
        test('creates complete directory structure', async () => {
            await artifact.createStructure();
            
            // Check artifact directory exists
            expect(await artifact.exists(artifact.artifactPath)).toBe(true);
            
            // Check required directories exist
            const directories = artifact.getRequiredDirectories();
            for (const dir of directories) {
                const dirPath = path.join(artifact.artifactPath, dir);
                expect(await artifact.exists(dirPath)).toBe(true);
            }
        });
        
        test('creates required base files', async () => {
            await artifact.createStructure();
            
            // Check artifact.json exists and is valid
            const jsonPath = path.join(artifact.artifactPath, 'artifact.json');
            expect(await artifact.exists(jsonPath)).toBe(true);
            
            const jsonContent = await fs.readFile(jsonPath, 'utf-8');
            const metadata = JSON.parse(jsonContent);
            expect(metadata.type).toBe('test_type');
            expect(metadata.id).toBe('test_id');
            
            // Check summary.txt exists
            const summaryPath = path.join(artifact.artifactPath, 'summary.txt');
            expect(await artifact.exists(summaryPath)).toBe(true);
        });
    });
    
    describe('Logging Interface', () => {
        beforeEach(async () => {
            await artifact.createStructure();
        });
        
        test('logs client messages correctly', async () => {
            await artifact.logClient('Test client message');
            
            const logPath = path.join(artifact.artifactPath, 'logs', 'client.log');
            const logContent = await fs.readFile(logPath, 'utf-8');
            expect(logContent).toContain('Test client message');
        });
        
        test('logs server messages correctly', async () => {
            await artifact.logServer('Test server message');
            
            const logPath = path.join(artifact.artifactPath, 'logs', 'server.log');
            const logContent = await fs.readFile(logPath, 'utf-8');
            expect(logContent).toContain('Test server message');
        });
        
        test('logs console messages correctly', async () => {
            await artifact.logConsole('Test console message');
            
            const logPath = path.join(artifact.artifactPath, 'logs', 'console.log');
            const logContent = await fs.readFile(logPath, 'utf-8');
            expect(logContent).toContain('Test console message');
        });
        
        test('logs commands correctly', async () => {
            await artifact.logCommand('test_command', 'SUCCESS');
            
            const logPath = path.join(artifact.artifactPath, 'logs', 'commands.log');
            const logContent = await fs.readFile(logPath, 'utf-8');
            expect(logContent).toContain('test_command -> SUCCESS');
        });
        
        test('logs errors to both errors.log and source log', async () => {
            await artifact.logError('Test error', 'client');
            
            // Check errors.log
            const errorsPath = path.join(artifact.artifactPath, 'logs', 'errors.log');
            const errorsContent = await fs.readFile(errorsPath, 'utf-8');
            expect(errorsContent).toContain('ERROR: Test error');
            
            // Check client.log also got the error
            const clientPath = path.join(artifact.artifactPath, 'logs', 'client.log');
            const clientContent = await fs.readFile(clientPath, 'utf-8');
            expect(clientContent).toContain('ERROR: Test error');
        });
    });
    
    describe('Status Management', () => {
        beforeEach(async () => {
            await artifact.createStructure();
        });
        
        test('updates status correctly', async () => {
            await artifact.updateStatus('RUNNING');
            
            expect(artifact.status).toBe('RUNNING');
            expect(artifact.metadata.status).toBe('RUNNING');
            
            // Check it was written to artifact.json
            const jsonPath = path.join(artifact.artifactPath, 'artifact.json');
            const jsonContent = await fs.readFile(jsonPath, 'utf-8');
            const metadata = JSON.parse(jsonContent);
            expect(metadata.status).toBe('RUNNING');
        });
        
        test('updates summary correctly', async () => {
            await artifact.writeSummary('Test summary');
            
            expect(artifact.metadata.summary).toBe('Test summary');
            
            // Check summary.txt
            const summaryPath = path.join(artifact.artifactPath, 'summary.txt');
            const summaryContent = await fs.readFile(summaryPath, 'utf-8');
            expect(summaryContent).toBe('Test summary');
            
            // Check artifact.json was also updated
            const jsonPath = path.join(artifact.artifactPath, 'artifact.json');
            const jsonContent = await fs.readFile(jsonPath, 'utf-8');
            const metadata = JSON.parse(jsonContent);
            expect(metadata.summary).toBe('Test summary');
        });
    });
    
    describe('Screenshot Management', () => {
        beforeEach(async () => {
            await artifact.createStructure();
        });
        
        test('adds screenshot correctly', async () => {
            // Create a test source file
            const sourceFile = path.join(tempDir, 'test_screenshot.png');
            await fs.writeFile(sourceFile, 'fake png content');
            
            const screenshotPath = await artifact.addScreenshot('test.png', sourceFile);
            
            expect(await artifact.exists(screenshotPath)).toBe(true);
            const content = await fs.readFile(screenshotPath, 'utf-8');
            expect(content).toBe('fake png content');
        });
    });
    
    describe('Self-Description', () => {
        test('provides structure manifest for widgets', () => {
            const manifest = artifact.getStructureManifest();
            
            expect(manifest.type).toBe('test_type');
            expect(manifest.artifactPath).toBe(artifact.artifactPath);
            expect(manifest.directories).toHaveLength(2); // logs, screenshots
            expect(manifest.files).toHaveLength(2); // artifact.json, summary.txt
            
            // Check directory descriptions
            const logsDir = manifest.directories.find(d => d.name === 'logs');
            expect(logsDir.purpose).toBe('System and application log files');
        });
    });
});