#!/usr/bin/env node

const { Anthropic } = require('@anthropic-ai/sdk');
const fs = require('fs');
const path = require('path');
const os = require('os');

async function testWeatherDirect() {
  console.log('🧪 DIRECT WEATHER FORMULA TEST');
  console.log('==============================');
  
  // Load API keys
  const userConfigFile = path.join(os.homedir(), '.continuum', 'config.env');
  if (fs.existsSync(userConfigFile)) {
    const configContent = fs.readFileSync(userConfigFile, 'utf-8');
    configContent.split('\n').forEach(line => {
      if (line.trim() && !line.startsWith('#')) {
        const [key, value] = line.split('=');
        if (key && value) {
          process.env[key.trim()] = value.trim();
        }
      }
    });
  }
  
  // Load commands from single source
  const commands = fs.readFileSync('./COMMANDS.md', 'utf-8');
  console.log('✅ Commands loaded from COMMANDS.md');
  
  // Test AI response
  const anthropic = new Anthropic({
    apiKey: process.env.ANTHROPIC_API_KEY
  });
  
  const prompt = `You are GeneralAI. Follow the protocol exactly.

${commands}

User Question: "How's the weather where I am?"

Follow the weather-formula pattern:
1. Get location first
2. Get weather data  
3. Respond to user

Remember: Use [STATUS], [CMD:ACTION], [CHAT] format. Commands are invisible to user.`;

  console.log('\n🤖 Sending to AI...');
  
  try {
    const response = await anthropic.messages.create({
      model: 'claude-3-5-sonnet-20241022',
      max_tokens: 1000,
      messages: [{ role: 'user', content: prompt }]
    });
    
    const aiResponse = response.content[0].text;
    
    console.log('\n📝 AI Response:');
    console.log('='.repeat(40));
    console.log(aiResponse);
    console.log('='.repeat(40));
    
    // Check if AI used proper protocol
    if (aiResponse.includes('[CMD:')) {
      console.log('\n✅ AI is using proper [CMD:] protocol!');
      
      // Extract commands
      const cmdMatches = aiResponse.matchAll(/\[CMD:(\w+)\]\s*([^\[\n]+)/g);
      let commandCount = 0;
      for (const match of cmdMatches) {
        commandCount++;
        console.log(`📤 Command ${commandCount}: ${match[1]} - ${match[2]}`);
      }
      
      if (commandCount > 0) {
        console.log('\n🎯 SUCCESS: AI is following command protocol!');
      }
    } else {
      console.log('\n❌ AI is NOT using proper protocol format');
    }
    
  } catch (error) {
    console.error('💥 Test failed:', error.message);
  }
}

testWeatherDirect();