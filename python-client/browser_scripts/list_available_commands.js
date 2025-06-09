// List Available Bus Commands
console.log("📋 Checking available bus commands for file operations...");

// We know the server reported 34 commands available
// Let's see if we can get the command list

// Try different ways to get command information
const commandQueries = [
    '[CMD:HELP]',
    '[CMD:LIST]', 
    '[CMD:COMMANDS]',
    '[CMD:LIST_COMMANDS]',
    '[CMD:AVAILABLE_COMMANDS]'
];

console.log("🔍 Trying to get command list...");

for (const query of commandQueries) {
    console.log("📤 Trying:", query);
    
    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
        const message = {
            type: 'task',
            role: 'system',
            task: query
        };
        
        window.ws.send(JSON.stringify(message));
    }
}

// Also check if there are any file-related commands by pattern
const fileCommandPatterns = [
    'SAVE',
    'WRITE', 
    'FILE',
    'EXPORT',
    'STORE',
    'SCREENSHOT'
];

console.log("🔍 Looking for file-related command patterns...");
console.log("   Patterns to look for:", fileCommandPatterns);

// Check if we can find command registry or help
if (window.commandRegistry) {
    console.log("✅ Found command registry");
    const commands = Object.keys(window.commandRegistry);
    const fileCommands = commands.filter(cmd => 
        fileCommandPatterns.some(pattern => cmd.toUpperCase().includes(pattern))
    );
    console.log("📁 File-related commands found:", fileCommands);
} else {
    console.log("❌ No command registry found in window");
}

// Check for continuum-specific objects
if (window.continuum) {
    console.log("✅ Found continuum object");
    if (window.continuum.commands) {
        console.log("📋 Continuum commands available");
    }
    if (window.continuum.commandProcessor) {
        console.log("🔧 Command processor available");
    }
} else {
    console.log("❌ No continuum object found");
}

// Return status
JSON.stringify({
    status: "COMMAND_LIST_REQUESTED",
    queries_sent: commandQueries,
    file_patterns: fileCommandPatterns,
    has_command_registry: !!window.commandRegistry,
    has_continuum: !!window.continuum,
    timestamp: Date.now()
});