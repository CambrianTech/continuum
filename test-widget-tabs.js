// Test script to verify widget tab switching functionality
// Run this in the browser console to test tab preservation

console.log('🧪 Testing widget tab switching functionality...');

// First, let's check if everything is loaded
window.testWidgetTabs = function() {

// Find the sidebar widget
const sidebar = document.querySelector('continuum-sidebar');
if (!sidebar) {
    console.error('❌ Sidebar widget not found');
    return;
}

console.log('✅ Found sidebar widget');

// Get the shadow root
const shadowRoot = sidebar.shadowRoot;
if (!shadowRoot) {
    console.error('❌ Shadow root not found');
    return;
}

// Find room tabs
const tabs = shadowRoot.querySelectorAll('.room-tab');
console.log(`✅ Found ${tabs.length} room tabs`);

// Find persona widgets before tab switch
const personasBefore = shadowRoot.querySelectorAll('persona-widget');
console.log(`📊 Before switch: ${personasBefore.length} persona widgets`);

// Log content of first persona widget before switch
if (personasBefore.length > 0) {
    const firstPersona = personasBefore[0];
    const hasConfig = firstPersona.config !== null;
    console.log(`📊 First persona configured: ${hasConfig}`);
    if (hasConfig) {
        console.log(`📊 First persona name: ${firstPersona.config.name}`);
    } else {
        console.log('📊 First persona shows: No Persona Configured');
    }
}

// Function to simulate tab click
function clickTab(tabName) {
    const targetTab = Array.from(tabs).find(tab => tab.dataset.room === tabName);
    if (targetTab) {
        console.log(`🖱️ Clicking ${tabName} tab...`);
        targetTab.click();
        
        // Check widgets after switch
        setTimeout(() => {
            const personasAfter = shadowRoot.querySelectorAll('persona-widget');
            console.log(`📊 After switch to ${tabName}: ${personasAfter.length} persona widgets`);
            
            if (personasAfter.length > 0) {
                const firstPersonaAfter = personasAfter[0];
                const hasConfigAfter = firstPersonaAfter.config !== null;
                console.log(`📊 First persona still configured: ${hasConfigAfter}`);
                if (hasConfigAfter) {
                    console.log(`📊 First persona name: ${firstPersonaAfter.config.name}`);
                } else {
                    console.log('❌ First persona lost config - shows: No Persona Configured');
                }
            }
        }, 500);
    } else {
        console.error(`❌ ${tabName} tab not found`);
    }
}

// Test sequence: General -> Academy -> Projects -> General
console.log('🧪 Starting tab switching test sequence...');

setTimeout(() => clickTab('academy'), 1000);
setTimeout(() => clickTab('projects'), 2000);
setTimeout(() => clickTab('general'), 3000);

console.log('🧪 Test sequence initiated. Watch logs above for results.');

}; // End of testWidgetTabs function

// Auto-run the test
console.log('🧪 Widget tab testing script loaded. Call testWidgetTabs() to run test.');