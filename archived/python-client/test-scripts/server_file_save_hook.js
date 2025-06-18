// Server File Save Hook - Add to Continuum Command Processor
// This should be added to the server-side command registry

const fs = require('fs').promises;
const path = require('path');

// Add SAVE_FILE command to the command processor
commands['SAVE_FILE'] = async (params) => {
    console.log("💾 [SAVE_FILE] Command received");
    console.log("   📤 Raw params length:", params.length, "chars");
    
    try {
        const data = JSON.parse(params);
        console.log("   📋 Parsed data:");
        console.log("      🏷️ Filename:", data.filename);
        console.log("      📁 Directory:", data.directory);
        console.log("      🎯 MIME type:", data.mimeType);
        console.log("      📊 Content length:", data.content?.length || 0, "chars");
        console.log("      🔍 Metadata:", JSON.stringify(data.metadata || {}, null, 2));
        
        // Validate required fields
        if (!data.filename || !data.directory || !data.content) {
            console.log("   ❌ Missing required fields");
            return { success: false, error: "Missing filename, directory, or content" };
        }
        
        // Construct full file path
        const fullPath = path.join(data.directory, data.filename);
        console.log("   🎯 FULL FILE PATH:", fullPath);
        console.log("   📍 Absolute path:", path.resolve(fullPath));
        
        // Create directory if needed
        console.log("   📁 Ensuring directory exists:", data.directory);
        await fs.mkdir(data.directory, { recursive: true });
        console.log("   ✅ Directory ready");
        
        // Decode base64 content
        console.log("   🔄 Decoding base64 content...");
        const buffer = Buffer.from(data.content, 'base64');
        console.log("   📊 Decoded buffer size:", buffer.length, "bytes");
        
        // LOG IMMEDIATELY BEFORE SAVE
        console.log("   🚀 ABOUT TO SAVE FILE:");
        console.log("      📍 Path:", fullPath);
        console.log("      💾 Size:", buffer.length, "bytes");
        console.log("      ⏰ Timestamp:", new Date().toISOString());
        
        // Write file to filesystem
        await fs.writeFile(fullPath, buffer);
        
        // Verify file was written
        const stats = await fs.stat(fullPath);
        console.log("   ✅ FILE SAVED SUCCESSFULLY!");
        console.log("      📁 Final path:", fullPath);
        console.log("      📊 File size on disk:", stats.size, "bytes");
        console.log("      📅 Created:", stats.birthtime);
        console.log("      📝 Modified:", stats.mtime);
        
        // Log success summary
        console.log("   🎉 SAVE_FILE COMPLETE:");
        console.log("      ✅ Success: true");
        console.log("      🏷️ File:", data.filename);
        console.log("      📁 Location:", data.directory);
        console.log("      💾 Bytes written:", stats.size);
        
        return {
            success: true,
            filename: data.filename,
            fullPath: fullPath,
            size: stats.size,
            created: stats.birthtime,
            message: `File saved successfully to ${fullPath}`
        };
        
    } catch (error) {
        console.log("   ❌ SAVE_FILE ERROR:");
        console.log("      🚨 Error type:", error.constructor.name);
        console.log("      📝 Error message:", error.message);
        console.log("      📍 Error stack:", error.stack);
        console.log("      📊 Params received:", params.substring(0, 200) + "...");
        
        return {
            success: false,
            error: error.message,
            errorType: error.constructor.name
        };
    }
};

// Log command registration
console.log("🔧 SAVE_FILE command registered in processor");
console.log("   📋 Total commands available:", Object.keys(commands).length);
console.log("   🎯 Server ready to receive file save requests");
console.log("   📁 Will save files to .continuum/screenshots/ and other directories");

// Export for integration
module.exports = { SAVE_FILE: commands['SAVE_FILE'] };