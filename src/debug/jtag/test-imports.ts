// Test file to verify path mappings work with tsx
import { ChatCommandBase } from '@chatShared/ChatCommandBase';
import { CommandBase } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';

console.log('✅ All imports successful!');
console.log('   📁 ChatCommandBase:', typeof ChatCommandBase);
console.log('   📁 CommandBase:', typeof CommandBase);
console.log('   📁 JTAGContext type imported');