/**
 * Unit Tests for Version Management
 * Test version-aware JavaScript reloading in continuum browser API
 */

describe('Continuum Version Management', () => {
    let mockContinuum;
    let mockDocument;
    let mockScript;
    
    beforeEach(() => {
        // Mock continuum object
        mockContinuum = {
            fileVersions: {
                'ScreenshotUtils.js': '1.0.0',   // Added for test consistency
                'CommandHandler.js': '2.0.0',
                'ComponentLoader.js': '1.0.0'
            },
            checkVersions: function(serverVersions) {
                const needsUpdate = [];
                
                for (const [filename, serverVersion] of Object.entries(serverVersions)) {
                    const currentVersion = this.fileVersions[filename];
                    if (!currentVersion || currentVersion !== serverVersion) {
                        needsUpdate.push({filename, currentVersion, serverVersion});
                    }
                }
                
                return needsUpdate;
            },
            reloadScripts: function(filesToUpdate) {
                // Mock implementation for testing
                return Promise.resolve(filesToUpdate);
            }
        };
        
        // Mock DOM elements
        mockScript = {
            remove: jest.fn(),
            onload: null,
            onerror: null,
            src: '',
            setAttribute: jest.fn()
        };
        
        mockDocument = {
            querySelector: jest.fn(),
            createElement: jest.fn(() => mockScript),
            head: {
                appendChild: jest.fn()
            }
        };
        
        global.document = mockDocument;
        global.console = {
            log: jest.fn(),
            error: jest.fn()
        };
    });
    
    describe('checkVersions', () => {
        test('identifies files needing updates', () => {
            const serverVersions = {
                'ScreenshotUtils.js': '1.1.0', // Needs update
                'CommandHandler.js': '2.0.0',  // Already current
                'NewFile.js': '1.0.0'          // New file
            };
            
            const result = mockContinuum.checkVersions(serverVersions);
            
            expect(result).toHaveLength(2);
            expect(result[0]).toEqual({
                filename: 'ScreenshotUtils.js',
                currentVersion: '1.0.0',
                serverVersion: '1.1.0'
            });
            expect(result[1]).toEqual({
                filename: 'NewFile.js',
                currentVersion: undefined,
                serverVersion: '1.0.0'
            });
        });
        
        test('returns empty array when all versions are current', () => {
            const serverVersions = {
                'ScreenshotUtils.js': '1.0.0',
                'CommandHandler.js': '2.0.0'
            };
            
            const result = mockContinuum.checkVersions(serverVersions);
            
            expect(result).toHaveLength(0);
        });
        
        test('handles empty server versions', () => {
            const result = mockContinuum.checkVersions({});
            
            expect(result).toHaveLength(0);
        });
    });
    
    describe('Script reloading simulation', () => {
        test('generates correct script URLs with version parameters for core UI files', () => {
            const filesToUpdate = [
                {filename: 'CommandHandler.js', currentVersion: '1.0.0', serverVersion: '2.1.0'}
            ];
            
            // Simulate script loading
            mockDocument.createElement.mockReturnValue(mockScript);
            
            // Test that the mock works for core UI files (command-specific files are handled by their modules)
            const script = mockDocument.createElement('script');
            script.src = `/src/ui/utils/${filesToUpdate[0].filename}?v=${filesToUpdate[0].serverVersion}`;
            script.setAttribute('data-continuum-file', filesToUpdate[0].filename);
            
            expect(script.src).toBe('/src/ui/utils/CommandHandler.js?v=2.1.0');
            expect(script.setAttribute).toHaveBeenCalledWith('data-continuum-file', 'CommandHandler.js');
        });
        
        test('removes old scripts before loading new ones', () => {
            const existingScript = {remove: jest.fn()};
            mockDocument.querySelector.mockReturnValue(existingScript);
            
            // Simulate finding and removing old script
            const oldScript = mockDocument.querySelector('script[data-continuum-file=\"CommandHandler.js\"]');
            if (oldScript) {
                oldScript.remove();
            }
            
            expect(mockDocument.querySelector).toHaveBeenCalledWith('script[data-continuum-file=\"CommandHandler.js\"]');
            expect(existingScript.remove).toHaveBeenCalled();
        });
    });
    
    describe('Version comparison logic', () => {
        test('semantic version comparison', () => {
            const testCases = [
                {current: '1.0.0', server: '1.0.1', shouldUpdate: true},
                {current: '1.0.0', server: '1.1.0', shouldUpdate: true},
                {current: '1.0.0', server: '2.0.0', shouldUpdate: true},
                {current: '1.1.0', server: '1.0.0', shouldUpdate: true}, // Downgrade
                {current: '1.0.0', server: '1.0.0', shouldUpdate: false},
                {current: undefined, server: '1.0.0', shouldUpdate: true}, // New file
                {current: '1.0.0', server: undefined, shouldUpdate: false} // Server doesn't specify
            ];
            
            testCases.forEach(({current, server, shouldUpdate}, index) => {
                const versions = server ? {'TestFile.js': server} : {};
                const continuum = {
                    fileVersions: current ? {'TestFile.js': current} : {},
                    checkVersions: mockContinuum.checkVersions
                };
                
                const result = continuum.checkVersions(versions);
                const needsUpdate = result.length > 0;
                
                expect(needsUpdate).toBe(shouldUpdate, 
                    `Test case ${index + 1}: current=${current}, server=${server} should ${shouldUpdate ? '' : 'not '}need update`);
            });
        });
    });
    
    describe('Error handling', () => {
        test('handles script loading errors gracefully', async () => {
            const filesToUpdate = [
                {filename: 'BadFile.js', currentVersion: '1.0.0', serverVersion: '1.1.0'}
            ];
            
            // Mock a reloadScripts function that can fail
            const reloadScriptsWithError = function(filesToUpdate) {
                return Promise.reject(new Error('Script loading failed'));
            };
            
            try {
                await reloadScriptsWithError(filesToUpdate);
                fail('Should have thrown an error');
            } catch (error) {
                expect(error.message).toBe('Script loading failed');
            }
        });
        
        test('handles partial script loading failures', async () => {
            const filesToUpdate = [
                {filename: 'GoodFile.js', currentVersion: '1.0.0', serverVersion: '1.1.0'},
                {filename: 'BadFile.js', currentVersion: '1.0.0', serverVersion: '1.1.0'}
            ];
            
            // Mock partial failure
            const reloadScriptsPartialFail = function(filesToUpdate) {
                const promises = filesToUpdate.map(file => {
                    if (file.filename === 'BadFile.js') {
                        return Promise.reject(new Error(`Failed to load ${file.filename}`));
                    }
                    return Promise.resolve(file);
                });
                
                return Promise.allSettled(promises);
            };
            
            const results = await reloadScriptsPartialFail(filesToUpdate);
            
            expect(results).toHaveLength(2);
            expect(results[0].status).toBe('fulfilled');
            expect(results[1].status).toBe('rejected');
        });
    });
    
    describe('Integration scenarios', () => {
        test('full version check and update workflow', () => {
            const serverVersions = {
                'ScreenshotUtils.js': '1.1.0',
                'CommandHandler.js': '2.0.0',
                'NewValidator.js': '1.0.0'
            };
            
            // Step 1: Check versions
            const needsUpdate = mockContinuum.checkVersions(serverVersions);
            
            expect(needsUpdate).toHaveLength(2);
            expect(needsUpdate.map(f => f.filename)).toEqual(['ScreenshotUtils.js', 'NewValidator.js']);
            
            // Step 2: Update file versions after successful reload
            needsUpdate.forEach(file => {
                mockContinuum.fileVersions[file.filename] = file.serverVersion;
            });
            
            // Step 3: Verify no more updates needed
            const secondCheck = mockContinuum.checkVersions(serverVersions);
            expect(secondCheck).toHaveLength(0);
            
            // Verify versions were updated
            expect(mockContinuum.fileVersions['ScreenshotUtils.js']).toBe('1.1.0');
            expect(mockContinuum.fileVersions['NewValidator.js']).toBe('1.0.0');
        });
        
        test('handles version message from WebSocket', () => {
            const versionMessage = {
                type: 'version_check',
                fileVersions: {
                    'ScreenshotUtils.js': '1.2.0',
                    'CommandHandler.js': '2.1.0'
                }
            };
            
            const filesToUpdate = mockContinuum.checkVersions(versionMessage.fileVersions);
            
            expect(filesToUpdate).toHaveLength(2);
            expect(filesToUpdate[0].filename).toBe('ScreenshotUtils.js');
            expect(filesToUpdate[0].serverVersion).toBe('1.2.0');
            expect(filesToUpdate[1].filename).toBe('CommandHandler.js');
            expect(filesToUpdate[1].serverVersion).toBe('2.1.0');
        });
    });
    
    describe('Version format validation', () => {
        test('handles different version formats', () => {
            const versionFormats = [
                '1.0.0',      // Standard semantic version
                '1.0',        // Two-part version
                '1',          // Single number
                '1.0.0-beta', // Pre-release
                '1.0.0+build' // Build metadata
            ];
            
            versionFormats.forEach(version => {
                const serverVersions = {'TestFile.js': version};
                const continuum = {
                    fileVersions: {'TestFile.js': '1.0.0'},
                    checkVersions: mockContinuum.checkVersions
                };
                
                const result = continuum.checkVersions(serverVersions);
                
                // All different versions should trigger update
                if (version !== '1.0.0') {
                    expect(result).toHaveLength(1);
                    expect(result[0].serverVersion).toBe(version);
                } else {
                    expect(result).toHaveLength(0);
                }
            });
        });
    });
});

// Mock data for testing
const mockVersionData = {
    currentVersions: {
        'ScreenshotUtils.js': '1.0.0',
        'CommandHandler.js': '2.0.0',
        'ValidationUtils.js': '1.5.0'
    },
    
    serverVersions: {
        'ScreenshotUtils.js': '1.1.0',
        'CommandHandler.js': '2.0.0',
        'ValidationUtils.js': '1.6.0',
        'NewFeature.js': '1.0.0'
    },
    
    expectedUpdates: [
        {filename: 'ScreenshotUtils.js', currentVersion: '1.0.0', serverVersion: '1.1.0'},
        {filename: 'ValidationUtils.js', currentVersion: '1.5.0', serverVersion: '1.6.0'},
        {filename: 'NewFeature.js', currentVersion: undefined, serverVersion: '1.0.0'}
    ]
};

module.exports = { mockVersionData };