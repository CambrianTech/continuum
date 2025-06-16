/**
 * Unit Tests for Promise-Based API
 * Test that validation errors are properly rejected as promises across all clients
 */

describe('Promise-Based API Validation', () => {
    let mockContinuum;
    let mockScreenshotUtils;
    
    beforeEach(() => {
        // Mock console
        global.console = {
            log: jest.fn(),
            warn: jest.fn(),
            error: jest.fn()
        };
        
        // Mock DOM elements and queries
        global.document = {
            body: {
                tagName: 'BODY',
                offsetWidth: 1673,
                offsetHeight: 1630,
                querySelectorAll: jest.fn(() => {
                    // Return 39 zero-dimension elements to trigger validation
                    const mockElements = [];
                    for (let i = 0; i < 39; i++) {
                        mockElements.push({
                            tagName: i < 5 ? 'BUTTON' : i < 28 ? 'DIV' : 'SCRIPT',
                            offsetWidth: 0,
                            offsetHeight: 0
                        });
                    }
                    return mockElements;
                })
            }
        };
        
        // Mock ScreenshotUtils with enhanced validation
        mockScreenshotUtils = {
            takeScreenshot: function(targetElement, options = {}) {
                const { source = 'unknown' } = options;
                
                console.log(`📸 Taking screenshot: ${source} (${targetElement.tagName})`);
                
                if (!targetElement) {
                    return Promise.reject(new Error('Target element is required'));
                }
                
                // Simple validation: reject if target element has zero dimensions
                if (targetElement.offsetWidth === 0 || targetElement.offsetHeight === 0) {
                    const error = `Cannot screenshot, element size is ${targetElement.offsetWidth}x${targetElement.offsetHeight}`;
                    console.error(`❌ ${error}`);
                    return Promise.reject(new Error(error));
                }
                
                // For large captures (like document.body), check for problematic children
                if (targetElement === document.body || targetElement.tagName === 'BODY') {
                    const zeroElements = targetElement.querySelectorAll('*');
                    let zeroCount = 0;
                    let canvasCount = 0;
                    
                    for (let element of zeroElements) {
                        if (element.offsetWidth === 0 || element.offsetHeight === 0) {
                            zeroCount++;
                            if (element.tagName === 'CANVAS') {
                                canvasCount++;
                            }
                            if (zeroCount > 50) break;
                        }
                    }
                    
                    // Be more aggressive for document.body - reject with clear message
                    if (zeroCount > 10) {
                        const error = `Cannot screenshot document.body, found ${zeroCount} elements with 0x0 dimensions (${canvasCount} canvas). Use a more specific selector like '#main-content' instead.`;
                        console.error(`❌ Enhanced validation: ${error}`);
                        return Promise.reject(new Error(error));
                    } else if (zeroCount > 0) {
                        console.warn(`⚠️ Found ${zeroCount} zero-dimension elements, attempting screenshot with filtering...`);
                    }
                }
                
                // Mock successful screenshot
                return Promise.resolve({
                    width: 800,
                    height: 600,
                    toDataURL: () => 'data:image/png;base64,mock-data'
                });
            }
        };
        
        global.window = {
            ScreenshotUtils: mockScreenshotUtils
        };
    });
    
    describe('Enhanced Validation Promise Rejection', () => {
        test('rejects promise with clear message for document.body', async () => {
            expect.assertions(3);
            
            try {
                await mockScreenshotUtils.takeScreenshot(document.body, {
                    scale: 0.3,
                    source: 'test_validation'
                });
                
                // Should not reach here
                fail('Screenshot should have been rejected');
            } catch (error) {
                expect(error).toBeInstanceOf(Error);
                expect(error.message).toContain('Cannot screenshot document.body');
                expect(error.message).toContain('Use a more specific selector');
            }
        });
        
        test('provides specific count of problematic elements', async () => {
            expect.assertions(2);
            
            try {
                await mockScreenshotUtils.takeScreenshot(document.body, {
                    source: 'count_test'
                });
                fail('Should have rejected');
            } catch (error) {
                expect(error.message).toContain('found 39 elements with 0x0 dimensions');
                expect(error.message).toContain('(0 canvas)');
            }
        });
        
        test('logs error to console before rejecting', async () => {
            expect.assertions(2);
            
            try {
                await mockScreenshotUtils.takeScreenshot(document.body, {
                    source: 'logging_test'
                });
                fail('Should have rejected');
            } catch (error) {
                expect(console.error).toHaveBeenCalledWith(
                    expect.stringContaining('❌ Enhanced validation:')
                );
                expect(console.error).toHaveBeenCalledWith(
                    expect.stringContaining('Cannot screenshot document.body')
                );
            }
        });
        
        test('succeeds for elements with valid dimensions', async () => {
            const validElement = {
                tagName: 'DIV',
                offsetWidth: 300,
                offsetHeight: 200,
                querySelectorAll: () => [] // No child elements
            };
            
            const result = await mockScreenshotUtils.takeScreenshot(validElement, {
                source: 'valid_test'
            });
            
            expect(result).toBeDefined();
            expect(result.width).toBe(800);
            expect(result.height).toBe(600);
            expect(typeof result.toDataURL).toBe('function');
        });
        
        test('rejects for zero-dimension elements', async () => {
            const zeroElement = {
                tagName: 'DIV',
                offsetWidth: 0,
                offsetHeight: 100
            };
            
            expect.assertions(1);
            
            try {
                await mockScreenshotUtils.takeScreenshot(zeroElement);
                fail('Should have rejected');
            } catch (error) {
                expect(error.message).toBe('Cannot screenshot, element size is 0x100');
            }
        });
    });
    
    describe('Promise Chain Compatibility', () => {
        test('works with .then().catch() syntax', (done) => {
            mockScreenshotUtils.takeScreenshot(document.body, {
                source: 'then_catch_test'
            })
            .then(() => {
                done.fail('Should not resolve');
            })
            .catch((error) => {
                expect(error.message).toContain('Cannot screenshot document.body');
                done();
            });
        });
        
        test('works with async/await syntax', async () => {
            let caughtError = null;
            
            try {
                await mockScreenshotUtils.takeScreenshot(document.body);
            } catch (error) {
                caughtError = error;
            }
            
            expect(caughtError).not.toBeNull();
            expect(caughtError.message).toContain('Cannot screenshot document.body');
        });
        
        test('works with Promise.all() for batch operations', async () => {
            const validElement = {
                tagName: 'DIV',
                offsetWidth: 300,
                offsetHeight: 200,
                querySelectorAll: () => []
            };
            
            const promises = [
                mockScreenshotUtils.takeScreenshot(validElement),
                mockScreenshotUtils.takeScreenshot(document.body).catch(e => ({ error: e.message })),
                mockScreenshotUtils.takeScreenshot(validElement)
            ];
            
            const results = await Promise.all(promises);
            
            expect(results[0]).toHaveProperty('width', 800); // Success
            expect(results[1]).toHaveProperty('error'); // Rejection caught
            expect(results[1].error).toContain('Cannot screenshot document.body');
            expect(results[2]).toHaveProperty('width', 800); // Success
        });
    });
    
    describe('Cross-Client Promise Behavior', () => {
        test('maintains error details across promise boundaries', async () => {
            const errorPromise = mockScreenshotUtils.takeScreenshot(document.body);
            
            // Simulate passing promise to another layer (like Python client)
            const clientResult = await errorPromise
                .then(canvas => ({
                    success: true,
                    canvas: canvas,
                    error: null
                }))
                .catch(error => ({
                    success: false,
                    canvas: null,
                    error: error.message
                }));
            
            expect(clientResult.success).toBe(false);
            expect(clientResult.error).toContain('Cannot screenshot document.body');
            expect(clientResult.error).toContain('found 39 elements with 0x0 dimensions');
            expect(clientResult.error).toContain('Use a more specific selector');
        });
        
        test('preserves stack trace information', async () => {
            try {
                await mockScreenshotUtils.takeScreenshot(document.body);
                fail('Should have rejected');
            } catch (error) {
                expect(error.stack).toBeDefined();
                expect(error.name).toBe('Error');
                expect(error.message).toContain('Cannot screenshot document.body');
            }
        });
        
        test('allows chaining of validation errors', async () => {
            const validationChain = mockScreenshotUtils
                .takeScreenshot(document.body)
                .catch(firstError => {
                    // Transform the error for a higher level
                    throw new Error(`Screenshot validation failed: ${firstError.message}`);
                })
                .catch(transformedError => {
                    // Log and re-throw for final handler
                    console.log('Final error handler:', transformedError.message);
                    return { finalError: transformedError.message };
                });
            
            const result = await validationChain;
            
            expect(result.finalError).toContain('Screenshot validation failed:');
            expect(result.finalError).toContain('Cannot screenshot document.body');
            expect(console.log).toHaveBeenCalledWith(
                'Final error handler:',
                expect.stringContaining('Screenshot validation failed:')
            );
        });
    });
    
    describe('API Level Features', () => {
        test('seamless promise propagation', async () => {
            // Simulate the full continuum API chain
            const continuumAPI = {
                command: {
                    screenshot: function(options) {
                        const { selector } = options;
                        const element = selector === 'body' ? document.body : { 
                            tagName: 'DIV', 
                            offsetWidth: 300, 
                            offsetHeight: 200,
                            querySelectorAll: () => []
                        };
                        return mockScreenshotUtils.takeScreenshot(element, options);
                    }
                }
            };
            
            // Test successful case
            const successResult = await continuumAPI.command.screenshot({
                selector: '#content',
                scale: 1.0
            });
            expect(successResult.width).toBe(800);
            
            // Test validation rejection
            let rejectionResult = null;
            try {
                await continuumAPI.command.screenshot({
                    selector: 'body',
                    scale: 0.3
                });
            } catch (error) {
                rejectionResult = error;
            }
            
            expect(rejectionResult).not.toBeNull();
            expect(rejectionResult.message).toContain('Cannot screenshot document.body');
        });
        
        test('universal command interface consistency', async () => {
            // Simulate different client types calling the same API
            const clientTypes = ['browser', 'python', 'ai-agent'];
            
            const results = await Promise.all(
                clientTypes.map(async (clientType) => {
                    try {
                        await mockScreenshotUtils.takeScreenshot(document.body, {
                            source: `${clientType}_test`
                        });
                        return { clientType, success: true, error: null };
                    } catch (error) {
                        return { clientType, success: false, error: error.message };
                    }
                })
            );
            
            // All client types should get the same validation error
            results.forEach(result => {
                expect(result.success).toBe(false);
                expect(result.error).toContain('Cannot screenshot document.body');
                expect(result.error).toContain('found 39 elements with 0x0 dimensions');
            });
            
            // Verify all got identical error messages
            const errorMessages = results.map(r => r.error);
            expect(new Set(errorMessages).size).toBe(1); // All identical
        });
    });
});

// Export test utilities for integration tests
export const promiseTestUtils = {
    createMockElement: (width, height, tagName = 'DIV') => ({
        tagName,
        offsetWidth: width,
        offsetHeight: height,
        querySelectorAll: () => []
    }),
    
    createProblematicBody: (zeroElementCount = 39) => ({
        tagName: 'BODY',
        offsetWidth: 1673,
        offsetHeight: 1630,
        querySelectorAll: () => {
            const elements = [];
            for (let i = 0; i < zeroElementCount; i++) {
                elements.push({
                    tagName: i % 2 === 0 ? 'DIV' : 'SCRIPT',
                    offsetWidth: 0,
                    offsetHeight: 0
                });
            }
            return elements;
        }
    }),
    
    expectValidationRejection: async (promise, expectedMessage) => {
        let error = null;
        try {
            await promise;
            throw new Error('Promise should have been rejected');
        } catch (e) {
            error = e;
        }
        
        expect(error).not.toBeNull();
        expect(error.message).toContain(expectedMessage);
        return error;
    }
};