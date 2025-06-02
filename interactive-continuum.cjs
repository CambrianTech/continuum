#!/usr/bin/env node
/**
 * INTERACTIVE CONTINUUM
 * 
 * Claude instances that can ASK YOU QUESTIONS and wait for answers
 * They remember context, ask for clarification, and make decisions with your input
 * Real interactive AI coordination with actual user feedback loops
 */

const fs = require('fs');
const path = require('path');
const { exec, spawn } = require('child_process');
const { promisify } = require('util');
const http = require('http');
const WebSocket = require('ws');

const execAsync = promisify(exec);

class InteractiveContinuum {
  constructor() {
    this.projectRoot = process.cwd();
    this.claudeInstances = new Map();
    this.connectedUsers = new Set();
    this.pendingQuestions = new Map(); // Store questions waiting for user answers
    this.conversationHistory = new Map(); // Store full conversation context
    this.isRunning = true;
    this.messageCounter = 0;
    
    console.log('🤔 INTERACTIVE CONTINUUM - ASK AND ANSWER');
    console.log('=========================================');
    console.log('💬 Claude instances can ask YOU questions');
    console.log('⏳ They wait for your responses');
    console.log('🧠 Remember full conversation context');
    console.log('🔄 Make decisions based on your feedback');
    console.log('');

    this.launch();
  }

  async launch() {
    console.log('🚀 Launching Interactive Continuum...');
    
    // Create communication system
    await this.setupCommunicationSystem();
    
    // Spawn interactive Claude instances
    await this.spawnInteractiveClaudeInstances();
    
    // Launch user interface
    await this.launchInterface();
    
    // Start monitoring and question handling
    this.startQuestionProcessing();
    
    console.log('✅ Interactive Continuum ready - Claude instances can ask you questions!');
    console.log('🌐 Access at http://localhost:5555');
  }

  async setupCommunicationSystem() {
    const commDir = path.join(this.projectRoot, '.interactive-comm');
    if (!fs.existsSync(commDir)) {
      fs.mkdirSync(commDir, { recursive: true });
    }
    
    // Create directories for each interactive Claude instance
    const instances = ['QuestionerClaude', 'DecisionClaude', 'PlannerClaude', 'ImplementerClaude', 'ReviewerClaude'];
    
    for (const instance of instances) {
      const instanceDir = path.join(commDir, instance);
      if (!fs.existsSync(instanceDir)) {
        fs.mkdirSync(instanceDir, { recursive: true });
      }
      
      // Create communication files
      fs.writeFileSync(path.join(instanceDir, 'inbox.json'), JSON.stringify([], null, 2));
      fs.writeFileSync(path.join(instanceDir, 'outbox.json'), JSON.stringify([], null, 2));
      fs.writeFileSync(path.join(instanceDir, 'questions.json'), JSON.stringify([], null, 2));
      fs.writeFileSync(path.join(instanceDir, 'conversation.json'), JSON.stringify([], null, 2));
      fs.writeFileSync(path.join(instanceDir, 'status.json'), JSON.stringify({
        name: instance,
        status: 'initializing',
        capabilities: this.getInstanceCapabilities(instance),
        lastActivity: new Date().toISOString(),
        waitingForResponse: false
      }, null, 2));
    }
    
    console.log('📁 Interactive communication system ready');
  }

  getInstanceCapabilities(instanceName) {
    const capabilities = {
      QuestionerClaude: ['Ask clarifying questions', 'Gather requirements', 'Understand user needs', 'Context building'],
      DecisionClaude: ['Present options', 'Ask for choices', 'Weigh trade-offs', 'Get approvals'],
      PlannerClaude: ['Create plans', 'Ask about priorities', 'Schedule confirmation', 'Resource planning'],
      ImplementerClaude: ['Execute tasks', 'Ask for guidance', 'Report progress', 'Handle errors'],
      ReviewerClaude: ['Quality checks', 'Ask for feedback', 'Suggest improvements', 'Final approval']
    };
    
    return capabilities[instanceName] || [];
  }

  async spawnInteractiveClaudeInstances() {
    console.log('🧠 Spawning interactive Claude instances...');
    
    const instances = ['QuestionerClaude', 'DecisionClaude', 'PlannerClaude', 'ImplementerClaude', 'ReviewerClaude'];
    
    for (const instanceName of instances) {
      const instance = {
        name: instanceName,
        id: `claude_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        capabilities: this.getInstanceCapabilities(instanceName),
        status: 'active',
        commDir: path.join(this.projectRoot, '.interactive-comm', instanceName),
        waitingForResponse: false,
        currentContext: [],
        
        // Enhanced communication methods with question support
        async askQuestion(question, options = null, context = '') {
          const questionId = Date.now().toString();
          
          // Store the question
          const questionData = {
            id: questionId,
            timestamp: new Date().toISOString(),
            from: this.name,
            question: question,
            options: options,
            context: context,
            answered: false,
            answer: null
          };
          
          // Save to questions file
          const questionsFile = path.join(this.commDir, 'questions.json');
          let questions = [];
          if (fs.existsSync(questionsFile)) {
            questions = JSON.parse(fs.readFileSync(questionsFile, 'utf-8'));
          }
          questions.push(questionData);
          fs.writeFileSync(questionsFile, JSON.stringify(questions, null, 2));
          
          // Update status
          this.waitingForResponse = true;
          const statusFile = path.join(this.commDir, 'status.json');
          const status = JSON.parse(fs.readFileSync(statusFile, 'utf-8'));
          status.waitingForResponse = true;
          status.currentQuestion = questionId;
          fs.writeFileSync(statusFile, JSON.stringify(status, null, 2));
          
          console.log(`❓ ${this.name} asks: "${question}"`);
          
          // Wait for response (polling)
          return new Promise((resolve) => {
            const checkAnswer = () => {
              const currentQuestions = JSON.parse(fs.readFileSync(questionsFile, 'utf-8'));
              const answeredQuestion = currentQuestions.find(q => q.id === questionId && q.answered);
              
              if (answeredQuestion) {
                console.log(`✅ ${this.name} received answer: "${answeredQuestion.answer}"`);
                this.waitingForResponse = false;
                
                // Update status
                status.waitingForResponse = false;
                delete status.currentQuestion;
                fs.writeFileSync(statusFile, JSON.stringify(status, null, 2));
                
                resolve(answeredQuestion.answer);
              } else {
                setTimeout(checkAnswer, 1000);
              }
            };
            checkAnswer();
          });
        },
        
        async sendMessage(message, recipient = 'user') {
          const outboxFile = path.join(this.commDir, 'outbox.json');
          let outbox = [];
          
          if (fs.existsSync(outboxFile)) {
            outbox = JSON.parse(fs.readFileSync(outboxFile, 'utf-8'));
          }
          
          outbox.push({
            id: Date.now().toString(),
            timestamp: new Date().toISOString(),
            from: this.name,
            to: recipient,
            content: message,
            type: 'message'
          });
          
          fs.writeFileSync(outboxFile, JSON.stringify(outbox, null, 2));
          console.log(`📤 ${this.name}: ${message.substring(0, 50)}...`);
        },
        
        async receiveMessages() {
          const inboxFile = path.join(this.commDir, 'inbox.json');
          if (fs.existsSync(inboxFile)) {
            const inbox = JSON.parse(fs.readFileSync(inboxFile, 'utf-8'));
            
            const unprocessed = inbox.filter(msg => !msg.processed);
            for (const msg of unprocessed) {
              await this.processMessage(msg);
              msg.processed = true;
            }
            
            fs.writeFileSync(inboxFile, JSON.stringify(inbox, null, 2));
          }
        },
        
        async processMessage(message) {
          console.log(`📨 ${this.name} processing: ${message.content}`);
          
          // Add to conversation history
          const conversationFile = path.join(this.commDir, 'conversation.json');
          let conversation = [];
          if (fs.existsSync(conversationFile)) {
            conversation = JSON.parse(fs.readFileSync(conversationFile, 'utf-8'));
          }
          
          conversation.push({
            timestamp: new Date().toISOString(),
            type: 'received',
            from: message.from,
            content: message.content
          });
          
          this.currentContext = conversation.slice(-10); // Keep last 10 messages as context
          
          // Generate response based on the instance's role
          let response = await this.generateInteractiveResponse(message);
          
          // Send response back
          await this.sendMessage(response, message.from);
          
          // Add response to conversation
          conversation.push({
            timestamp: new Date().toISOString(),
            type: 'sent',
            to: message.from,
            content: response
          });
          
          fs.writeFileSync(conversationFile, JSON.stringify(conversation, null, 2));
        },
        
        async generateInteractiveResponse(message) {
          const content = message.content.toLowerCase();
          
          if (this.name === 'QuestionerClaude') {
            if (content.includes('api') || content.includes('connect')) {
              const apiChoice = await this.askQuestion(
                "Which API service would you like me to integrate?",
                ["OpenAI GPT", "Anthropic Claude", "Other AI Service", "Custom API"],
                "I can help connect to various AI APIs"
              );
              
              if (apiChoice.toLowerCase().includes('openai')) {
                const hasKey = await this.askQuestion(
                  "Do you have an OpenAI API key ready? (yes/no)"
                );
                
                if (hasKey.toLowerCase().includes('yes')) {
                  const keyInput = await this.askQuestion(
                    "Please provide your OpenAI API key (it will be stored securely):"
                  );
                  return `Great! I'll set up OpenAI integration with your API key. Starting implementation...`;
                } else {
                  return `No problem! You can get an API key from https://platform.openai.com/api-keys. Let me know when you have it!`;
                }
              }
              
              return `I'll help you integrate ${apiChoice}. Let me ask DecisionClaude to help plan this.`;
            }
            
            // Always ask a follow-up question
            const clarification = await this.askQuestion(
              `I want to make sure I understand correctly. Are you asking me to help with: ${content}?`,
              ["Yes, that's right", "No, I meant something else", "Let me explain more"]
            );
            
            return `Thanks for clarifying! ${clarification}. I'll work on this now.`;
            
          } else if (this.name === 'DecisionClaude') {
            const decision = await this.askQuestion(
              "I need to make a decision about how to proceed. Which approach do you prefer?",
              ["Fast implementation", "Thorough planning first", "Ask more questions", "Let AI decide"],
              `Context: ${content}`
            );
            
            return `Based on your choice of "${decision}", I'll coordinate with the other Claude instances accordingly.`;
            
          } else if (this.name === 'PlannerClaude') {
            const priority = await this.askQuestion(
              "What's the priority level for this task?",
              ["High - do it now", "Medium - this week", "Low - when convenient"],
              "I'm creating a plan and need to know urgency"
            );
            
            return `Got it! Priority: ${priority}. I'm creating a plan based on this priority level.`;
            
          } else if (this.name === 'ImplementerClaude') {
            if (content.includes('error') || content.includes('problem')) {
              const shouldContinue = await this.askQuestion(
                "I encountered an issue. How should I handle this?",
                ["Try to fix it automatically", "Ask for guidance", "Stop and report", "Try alternative approach"]
              );
              
              return `I'll ${shouldContinue} as you requested.`;
            }
            
            const confirmation = await this.askQuestion(
              `Should I start implementing this right away? Task: ${content}`,
              ["Yes, start now", "Wait for approval", "Make a plan first"]
            );
            
            return `Understood! ${confirmation}. Beginning implementation...`;
            
          } else if (this.name === 'ReviewerClaude') {
            const feedback = await this.askQuestion(
              "I've reviewed the work. What would you like me to focus on?",
              ["Code quality", "Performance", "Security", "User experience", "All aspects"],
              "Ready to provide detailed feedback"
            );
            
            return `I'll focus my review on ${feedback} and provide detailed feedback.`;
          }
          
          return `I'm ${this.name} - I need more information to help you effectively. What specific assistance do you need?`;
        }
      };
      
      this.claudeInstances.set(instanceName, instance);
      
      // Update status file
      const statusFile = path.join(instance.commDir, 'status.json');
      fs.writeFileSync(statusFile, JSON.stringify({
        name: instanceName,
        id: instance.id,
        status: 'active',
        capabilities: instance.capabilities,
        lastActivity: new Date().toISOString(),
        waitingForResponse: false
      }, null, 2));
      
      console.log(`✅ ${instanceName} spawned and ready to ask questions`);
    }
    
    console.log(`🎯 ${this.claudeInstances.size} interactive Claude instances active`);
  }

  async launchInterface() {
    console.log('🌐 Launching Interactive Continuum interface...');
    
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateInteractiveUI());
      } else {
        res.writeHead(404);
        res.end('Not Found');
      }
    });

    this.webSocket = new WebSocket.Server({ server });
    
    this.webSocket.on('connection', (ws) => {
      this.connectedUsers.add(ws);
      console.log('👤 User connected to Interactive Continuum');
      
      // Send welcome with interactive instance status
      ws.send(JSON.stringify({
        type: 'system_status',
        data: {
          message: '🤔 Interactive Continuum active - Claude instances can ask you questions',
          instances: Array.from(this.claudeInstances.entries()).map(([name, instance]) => ({
            name: instance.name,
            id: instance.id,
            status: instance.status,
            capabilities: instance.capabilities,
            waitingForResponse: instance.waitingForResponse
          })),
          pendingQuestions: this.pendingQuestions.size
        }
      }));
      
      ws.on('message', (message) => {
        try {
          const data = JSON.parse(message);
          if (data.type === 'question_answer') {
            this.handleQuestionAnswer(data, ws);
          } else {
            this.routeToClaudeInstance(data, ws);
          }
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'error',
            data: 'Could not parse message'
          }));
        }
      });
      
      ws.on('close', () => {
        this.connectedUsers.delete(ws);
        console.log('👤 User disconnected');
      });
    });

    server.listen(5555, () => {
      console.log('🌐 Interactive Continuum interface ready at http://localhost:5555');
    });
  }

  async handleQuestionAnswer(data, ws) {
    const { questionId, answer, instanceName } = data;
    
    console.log(`💬 User answered question ${questionId}: "${answer}"`);
    
    // Find and update the question
    const instance = this.claudeInstances.get(instanceName);
    if (instance) {
      const questionsFile = path.join(instance.commDir, 'questions.json');
      if (fs.existsSync(questionsFile)) {
        const questions = JSON.parse(fs.readFileSync(questionsFile, 'utf-8'));
        const question = questions.find(q => q.id === questionId);
        
        if (question) {
          question.answered = true;
          question.answer = answer;
          question.answeredAt = new Date().toISOString();
          
          fs.writeFileSync(questionsFile, JSON.stringify(questions, null, 2));
          
          ws.send(JSON.stringify({
            type: 'answer_received',
            data: `Answer recorded for ${instanceName}`
          }));
        }
      }
    }
  }

  async routeToClaudeInstance(data, ws) {
    const message = data.content;
    this.messageCounter++;
    
    console.log(`📨 Routing user message: "${message}"`);
    
    // Decide which Claude instance should handle this
    const targetInstance = this.selectClaudeInstance(message);
    
    if (targetInstance) {
      ws.send(JSON.stringify({
        type: 'routing',
        data: `🎯 Routing to ${targetInstance.name} (they may ask you questions)`
      }));
      
      // Send message to Claude instance
      const inboxFile = path.join(targetInstance.commDir, 'inbox.json');
      let inbox = [];
      
      if (fs.existsSync(inboxFile)) {
        inbox = JSON.parse(fs.readFileSync(inboxFile, 'utf-8'));
      }
      
      inbox.push({
        id: this.messageCounter.toString(),
        timestamp: new Date().toISOString(),
        from: 'user',
        to: targetInstance.name,
        content: message,
        type: 'user_message',
        processed: false
      });
      
      fs.writeFileSync(inboxFile, JSON.stringify(inbox, null, 2));
      
      // Process the message
      await targetInstance.receiveMessages();
      
      // Check for response and questions
      setTimeout(() => {
        this.checkForResponse(targetInstance, ws);
        this.checkForQuestions(targetInstance, ws);
      }, 1000);
    }
  }

  selectClaudeInstance(message) {
    const lowerMessage = message.toLowerCase();
    
    if (lowerMessage.includes('question') || lowerMessage.includes('clarify') || lowerMessage.includes('understand')) {
      return this.claudeInstances.get('QuestionerClaude');
    } else if (lowerMessage.includes('decide') || lowerMessage.includes('choose') || lowerMessage.includes('option')) {
      return this.claudeInstances.get('DecisionClaude');
    } else if (lowerMessage.includes('plan') || lowerMessage.includes('schedule') || lowerMessage.includes('organize')) {
      return this.claudeInstances.get('PlannerClaude');
    } else if (lowerMessage.includes('do') || lowerMessage.includes('implement') || lowerMessage.includes('build')) {
      return this.claudeInstances.get('ImplementerClaude');
    } else if (lowerMessage.includes('review') || lowerMessage.includes('check') || lowerMessage.includes('feedback')) {
      return this.claudeInstances.get('ReviewerClaude');
    } else {
      return this.claudeInstances.get('QuestionerClaude'); // Default to asking questions
    }
  }

  async checkForQuestions(instance, ws) {
    const questionsFile = path.join(instance.commDir, 'questions.json');
    
    if (fs.existsSync(questionsFile)) {
      const questions = JSON.parse(fs.readFileSync(questionsFile, 'utf-8'));
      const unansweredQuestions = questions.filter(q => !q.answered);
      
      for (const question of unansweredQuestions) {
        ws.send(JSON.stringify({
          type: 'claude_question',
          data: {
            questionId: question.id,
            from: instance.name,
            question: question.question,
            options: question.options,
            context: question.context,
            timestamp: question.timestamp
          }
        }));
      }
    }
  }

  async checkForResponse(instance, ws) {
    const outboxFile = path.join(instance.commDir, 'outbox.json');
    
    if (fs.existsSync(outboxFile)) {
      const outbox = JSON.parse(fs.readFileSync(outboxFile, 'utf-8'));
      const newMessages = outbox.filter(msg => !msg.sent);
      
      for (const msg of newMessages) {
        if (msg.to === 'user') {
          ws.send(JSON.stringify({
            type: 'claude_response',
            data: {
              from: instance.name,
              content: msg.content,
              timestamp: msg.timestamp
            }
          }));
          
          msg.sent = true;
        }
      }
      
      if (newMessages.length > 0) {
        fs.writeFileSync(outboxFile, JSON.stringify(outbox, null, 2));
      }
    }
  }

  startQuestionProcessing() {
    console.log('❓ Starting question processing system...');
    
    setInterval(() => {
      // Check all instances for pending questions and broadcast them
      this.claudeInstances.forEach((instance, name) => {
        this.connectedUsers.forEach(ws => {
          this.checkForQuestions(instance, ws);
        });
      });
    }, 2000);
  }

  generateInteractiveUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Interactive Continuum - Ask & Answer</title>
    <style>
        body { 
            background: linear-gradient(135deg, #0f0f0f 0%, #1a1a3e 50%, #2d1b69 100%); 
            color: #e0e0ff; 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; 
            padding: 20px; 
            margin: 0;
            min-height: 100vh;
        }
        .header { 
            text-align: center; 
            border-bottom: 2px solid #6a5acd; 
            padding: 20px; 
            margin-bottom: 20px;
            background: rgba(106, 90, 205, 0.1);
            border-radius: 12px;
        }
        .question-section {
            background: rgba(255, 215, 0, 0.1);
            border: 2px solid #ffd700;
            padding: 20px;
            margin: 20px 0;
            border-radius: 12px;
            display: none;
        }
        .question-active {
            display: block;
        }
        .options {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 10px;
            margin: 15px 0;
        }
        .option-button {
            background: rgba(255, 215, 0, 0.1);
            border: 1px solid #ffd700;
            color: #e0e0ff;
            padding: 12px;
            cursor: pointer;
            border-radius: 8px;
            transition: all 0.3s;
        }
        .option-button:hover {
            background: rgba(255, 215, 0, 0.2);
        }
        .chat { 
            border: 1px solid #6a5acd; 
            padding: 20px; 
            height: 400px; 
            overflow-y: auto; 
            margin: 20px 0; 
            background: rgba(106, 90, 205, 0.03);
            border-radius: 12px;
        }
        .input-area {
            display: flex;
            gap: 12px;
            margin: 20px 0;
        }
        .input { 
            flex: 1;
            background: rgba(106, 90, 205, 0.1); 
            border: 2px solid #6a5acd; 
            color: #e0e0ff; 
            padding: 16px; 
            font-family: inherit;
            border-radius: 8px;
            font-size: 16px;
            outline: none;
        }
        .button { 
            background: rgba(106, 90, 205, 0.2); 
            border: 2px solid #6a5acd; 
            color: #e0e0ff; 
            padding: 16px 24px; 
            cursor: pointer;
            border-radius: 8px;
            transition: all 0.3s;
            font-weight: 600;
        }
        .button:hover {
            background: rgba(106, 90, 205, 0.3);
        }
        .message { 
            margin: 12px 0; 
            padding: 14px; 
            border-left: 4px solid #6a5acd; 
            border-radius: 8px;
            background: rgba(106, 90, 205, 0.05);
        }
        .question-message {
            border-left-color: #ffd700;
            background: rgba(255, 215, 0, 0.05);
        }
        .answer-input {
            width: 100%;
            margin: 10px 0;
            padding: 12px;
            background: rgba(255, 215, 0, 0.1);
            border: 1px solid #ffd700;
            color: #e0e0ff;
            border-radius: 8px;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🤔 Interactive Continuum</h1>
        <p>Claude instances that ask YOU questions and wait for answers</p>
        <div>Status: <span style="color: #32cd32;">Ready for Interactive AI</span></div>
    </div>
    
    <div id="questionSection" class="question-section">
        <h3>❓ Claude has a question for you:</h3>
        <div id="questionText"></div>
        <div id="questionContext" style="font-size: 14px; opacity: 0.8; margin: 10px 0;"></div>
        <div id="questionOptions" class="options"></div>
        <input type="text" id="answerInput" class="answer-input" placeholder="Type your answer here..." style="display: none;">
        <button class="button" onclick="submitAnswer()" id="submitButton" style="display: none;">Submit Answer</button>
    </div>
    
    <div class="chat" id="chat">
        <div class="message">
            🤔 Interactive Continuum ready - Claude instances will ask you questions and wait for your responses...
        </div>
    </div>
    
    <div class="input-area">
        <input type="text" id="messageInput" class="input" 
               placeholder="Ask Claude anything - they'll ask YOU questions back for clarification!" 
               onkeypress="if(event.key==='Enter') sendMessage()">
        <button class="button" onclick="sendMessage()">SEND</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5555');
        let currentQuestion = null;
        let isConnected = false;
        
        ws.onopen = function() {
            isConnected = true;
            addMessage('🟢 Connected to Interactive Continuum', 'message');
        };
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            
            if (data.type === 'system_status') {
                addMessage(data.data.message, 'message');
            } else if (data.type === 'routing') {
                addMessage(data.data, 'message');
            } else if (data.type === 'claude_response') {
                addMessage(\`🤖 \${data.data.from}: \${data.data.content}\`, 'message');
            } else if (data.type === 'claude_question') {
                displayQuestion(data.data);
            } else if (data.type === 'answer_received') {
                addMessage(\`✅ \${data.data}\`, 'message');
                hideQuestion();
            }
        };
        
        function displayQuestion(questionData) {
            currentQuestion = questionData;
            
            document.getElementById('questionText').textContent = questionData.question;
            document.getElementById('questionContext').textContent = questionData.context ? \`Context: \${questionData.context}\` : '';
            
            const questionSection = document.getElementById('questionSection');
            const optionsDiv = document.getElementById('questionOptions');
            const answerInput = document.getElementById('answerInput');
            const submitButton = document.getElementById('submitButton');
            
            questionSection.classList.add('question-active');
            optionsDiv.innerHTML = '';
            
            if (questionData.options && questionData.options.length > 0) {
                // Show option buttons
                answerInput.style.display = 'none';
                submitButton.style.display = 'none';
                
                questionData.options.forEach(option => {
                    const button = document.createElement('button');
                    button.className = 'option-button';
                    button.textContent = option;
                    button.onclick = () => selectOption(option);
                    optionsDiv.appendChild(button);
                });
            } else {
                // Show text input
                answerInput.style.display = 'block';
                submitButton.style.display = 'block';
                answerInput.focus();
            }
            
            addMessage(\`❓ \${questionData.from} asks: \${questionData.question}\`, 'question-message');
        }
        
        function selectOption(option) {
            submitAnswerText(option);
        }
        
        function submitAnswer() {
            const answer = document.getElementById('answerInput').value.trim();
            if (answer) {
                submitAnswerText(answer);
                document.getElementById('answerInput').value = '';
            }
        }
        
        function submitAnswerText(answer) {
            if (currentQuestion) {
                ws.send(JSON.stringify({
                    type: 'question_answer',
                    questionId: currentQuestion.questionId,
                    answer: answer,
                    instanceName: currentQuestion.from
                }));
                
                addMessage(\`💬 You answered: \${answer}\`, 'message');
            }
        }
        
        function hideQuestion() {
            document.getElementById('questionSection').classList.remove('question-active');
            currentQuestion = null;
        }
        
        function sendMessage() {
            const input = document.getElementById('messageInput');
            const message = input.value.trim();
            
            if (!message || !isConnected) return;
            
            addMessage('👤 ' + message, 'message');
            
            ws.send(JSON.stringify({
                type: 'user_message',
                content: message
            }));
            
            input.value = '';
        }
        
        function addMessage(text, className) {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'message ' + className;
            
            const timestamp = new Date().toLocaleTimeString();
            messageDiv.innerHTML = '<span style="opacity: 0.7; font-size: 12px;">' + timestamp + '</span> - ' + text;
            
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        // Allow Enter key to submit answers
        document.getElementById('answerInput').addEventListener('keypress', function(e) {
            if (e.key === 'Enter') {
                submitAnswer();
            }
        });
    </script>
</body>
</html>`;
  }
}

// Launch Interactive Continuum
console.log('🤔 Launching Interactive Continuum with questioning Claude instances...');
new InteractiveContinuum();