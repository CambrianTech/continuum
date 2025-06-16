/**
 * Test Scale Settings
 * Test the same element with different scale settings to see size differences
 */

console.log('🔍 Testing different scale settings for version badge...');

async function testScales() {
    const scales = [1.0, 2.0, 0.5];
    
    for (const scale of scales) {
        console.log(`📸 Testing scale: ${scale}`);
        
        try {
            const result = await window.continuum.command.screenshot({
                selector: '.version-badge',
                name_prefix: `scale_test_${scale}`,
                scale: scale
            });
            
            console.log(`✅ Scale ${scale} result:`, result);
        } catch (error) {
            console.error(`❌ Scale ${scale} failed:`, error.message);
        }
        
        // Wait between tests
        await new Promise(resolve => setTimeout(resolve, 1000));
    }
    
    console.log('🎯 Scale test complete');
}

// Check if continuum API is available
if (typeof window.continuum !== 'undefined' && window.continuum.command && window.continuum.command.screenshot) {
    testScales();
} else {
    console.error('❌ continuum.command.screenshot not available');
}