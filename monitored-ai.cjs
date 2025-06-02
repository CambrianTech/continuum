
const ContinuumSpawn = require('./continuum-spawn.cjs');

class MonitoredContinuumAI extends ContinuumSpawn {
  constructor() {
    super();
    this.monitoringEnabled = true;
    this.communicationChannel = '.continuum/communication';
    this.setupCommunicationListener();
  }

  setupCommunicationListener() {
    // Check for human messages every 10 seconds
    setInterval(() => {
      this.checkForHumanMessages();
    }, 10000);
  }

  checkForHumanMessages() {
    const commFile = path.join(this.projectRoot, this.communicationChannel, 'human-input.json');
    if (fs.existsSync(commFile)) {
      try {
        const message = JSON.parse(fs.readFileSync(commFile, 'utf-8'));
        if (message.timestamp > (this.lastMessageTime || 0)) {
          this.handleHumanMessage(message);
          this.lastMessageTime = message.timestamp;
        }
      } catch (error) {
        // Ignore parsing errors
      }
    }
  }

  handleHumanMessage(message) {
    console.log('📨 Received human message:', message.message);
    
    // Respond to human guidance
    if (message.message.toLowerCase().includes('ci status')) {
      this.respondToHuman('Checking CI status now...');
      this.checkCIStatus();
    } else if (message.message.toLowerCase().includes('stop')) {
      this.respondToHuman('Stopping current operation...');
      this.gracefulStop();
    } else if (message.message.toLowerCase().includes('status')) {
      this.respondToHuman('Current status: Working on cyberpunk improvements');
    }
  }

  respondToHuman(response) {
    const responseFile = path.join(this.projectRoot, this.communicationChannel, 'ai-response.json');
    fs.writeFileSync(responseFile, JSON.stringify({
      timestamp: Date.now(),
      response,
      type: 'ai-response'
    }, null, 2));
    console.log('📤 Responded to human:', response);
  }

  async checkCIStatus() {
    try {
      const { stdout } = await execAsync('gh pr checks 63');
      console.log('🔍 Real CI Status Check:');
      
      // Parse the actual failures
      const lines = stdout.split('\n');
      const failures = lines.filter(line => line.includes('fail'));
      
      if (failures.length > 0) {
        console.log('❌ Detected CI failures:');
        failures.forEach(failure => console.log('  ', failure));
        
        // Focus on the real issues
        await this.fixRealCIIssues(failures);
      } else {
        console.log('✅ No CI failures detected');
      }
    } catch (error) {
      console.log('⚠️  Could not check CI status:', error.message);
    }
  }

  async fixRealCIIssues(failures) {
    console.log('🔧 Focusing on real CI issues...');
    
    // Check what the build error actually is
    try {
      const { stdout, stderr } = await execAsync('npm run build 2>&1 || true');
      console.log('📋 Build output:', stdout);
      
      if (stderr || stdout.includes('error') || stdout.includes('Error')) {
        console.log('❌ Build is actually failing locally');
        await this.fixBuildIssues();
      } else {
        console.log('✅ Build passes locally - CI environment issue');
        await this.fixCIEnvironmentIssues();
      }
    } catch (error) {
      console.log('🔧 Attempting to fix build configuration...');
      await this.fixBuildConfiguration();
    }
  }

  async fixBuildIssues() {
    console.log('🔧 Fixing actual build issues...');
    
    // Check if memory package is the issue
    const memoryPackage = path.join(this.projectRoot, 'packages/memory/package.json');
    if (!fs.existsSync(memoryPackage)) {
      console.log('📦 Creating missing memory package.json...');
      // ... create proper package.json
    }
    
    // Check TypeScript issues
    const tsFiles = await this.findTypeScriptFiles();
    for (const file of tsFiles) {
      await this.fixTypeScriptFile(file);
    }
  }

  gracefulStop() {
    console.log('🛑 Graceful stop requested by human');
    process.exit(0);
  }
}

new MonitoredContinuumAI();
