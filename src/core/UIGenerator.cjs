/**
 * UI Generator
 * Generates the web interface HTML
 */

class UIGenerator {
  constructor(continuum) {
    this.continuum = continuum;
    // Initialize Academy interface
    const AcademyWebInterface = require('./AcademyWebInterface.cjs');
    this.academyInterface = new AcademyWebInterface(continuum);
  }

  generateHTML() {
    const packageInfo = require('../../package.json');
    
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum Academy - AI Workforce Construction</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🎓</text></svg>">
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif;
            background: linear-gradient(135deg, #0f1419 0%, #1a1f2e 100%);
            color: #e0e6ed;
            height: 100vh;
            overflow: hidden;
        }
        
        .app-container {
            display: flex;
            height: 100vh;
        }
        
        /* Sidebar */
        .sidebar {
            width: 300px;
            background: rgba(20, 25, 35, 0.95);
            backdrop-filter: blur(10px);
            border-right: 1px solid rgba(255, 255, 255, 0.1);
            display: flex;
            flex-direction: column;
        }
        
        /* Room Selection */
        .room-tabs {
            display: flex;
            margin: 20px;
            margin-bottom: 0;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 12px;
            padding: 4px;
        }
        
        .room-tab {
            flex: 1;
            padding: 10px 12px;
            text-align: center;
            border-radius: 8px;
            cursor: pointer;
            transition: all 0.3s ease;
            font-size: 14px;
            font-weight: 500;
            color: #8a92a5;
        }
        
        .room-tab.active {
            background: linear-gradient(135deg, #4FC3F7, #29B6F6);
            color: white;
            box-shadow: 0 2px 8px rgba(79, 195, 247, 0.3);
        }
        
        .room-tab:hover:not(.active) {
            background: rgba(255, 255, 255, 0.1);
            color: #e0e6ed;
        }
        
        .sidebar-header {
            padding: 20px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        }
        
        .logo {
            display: flex;
            align-items: center;
            margin-bottom: 10px;
        }
        
        .logo-icon {
            width: 32px;
            height: 32px;
            background: linear-gradient(135deg, #4FC3F7, #29B6F6);
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 12px;
            font-size: 18px;
        }
        
        .logo-text {
            font-size: 24px;
            font-weight: 700;
            color: #4FC3F7;
        }
        
        .subtitle {
            font-size: 14px;
            color: #8a92a5;
            margin-bottom: 15px;
        }
        
        .status-pill {
            display: inline-flex;
            align-items: center;
            padding: 6px 12px;
            background: rgba(76, 175, 80, 0.1);
            border: 1px solid rgba(76, 175, 80, 0.3);
            border-radius: 20px;
            font-size: 12px;
            color: #4CAF50;
        }
        
        .status-dot {
            width: 6px;
            height: 6px;
            background: #4CAF50;
            border-radius: 50%;
            margin-right: 8px;
            animation: pulse 2s infinite;
        }
        
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.5; }
        }
        
        /* Main Chat Area */
        .main-content {
            flex: 1;
            display: flex;
            flex-direction: column;
            background: rgba(15, 20, 25, 0.5);
        }
        
        .chat-header {
            padding: 20px 30px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
            background: rgba(20, 25, 35, 0.8);
            backdrop-filter: blur(10px);
        }
        
        .chat-title {
            font-size: 20px;
            font-weight: 600;
            color: #e0e6ed;
            margin-bottom: 5px;
        }
        
        .chat-subtitle {
            font-size: 14px;
            color: #8a92a5;
        }
        
        .chat-container {
            flex: 1;
            overflow-y: auto;
            padding: 20px 30px;
            scroll-behavior: smooth;
        }
        
        .chat-container::-webkit-scrollbar {
            width: 6px;
        }
        
        .chat-container::-webkit-scrollbar-track {
            background: transparent;
        }
        
        .chat-container::-webkit-scrollbar-thumb {
            background: rgba(255, 255, 255, 0.2);
            border-radius: 3px;
        }
        
        /* Message Bubbles */
        .message {
            margin-bottom: 20px;
            display: flex;
            flex-direction: column;
            max-width: 70%;
            animation: messageSlideIn 0.3s ease-out;
        }
        
        @keyframes messageSlideIn {
            from {
                opacity: 0;
                transform: translateY(10px);
            }
            to {
                opacity: 1;
                transform: translateY(0);
            }
        }
        
        .message.user {
            align-self: flex-end;
            align-items: flex-end;
        }
        
        .message.ai {
            align-self: flex-start;
            align-items: flex-start;
        }
        
        .message-sender {
            font-size: 12px;
            font-weight: 600;
            margin-bottom: 6px;
            color: #8a92a5;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .message.user .message-sender {
            color: #4FC3F7;
        }
        
        .message.ai .message-sender {
            color: #FFB74D;
        }
        
        .message-bubble {
            padding: 16px 20px;
            border-radius: 20px;
            position: relative;
            word-wrap: break-word;
            line-height: 1.5;
        }
        
        .message.user .message-bubble {
            background: linear-gradient(135deg, #1976D2, #1565C0);
            color: white;
            border-bottom-right-radius: 8px;
        }
        
        .message.ai .message-bubble {
            background: rgba(255, 255, 255, 0.08);
            border: 1px solid rgba(255, 255, 255, 0.1);
            color: #e0e6ed;
            border-bottom-left-radius: 8px;
        }
        
        .message-time {
            font-size: 11px;
            color: #666;
            margin-top: 6px;
        }
        
        /* Academy Training Status */
        .academy-training {
            background: linear-gradient(135deg, rgba(255, 183, 77, 0.1), rgba(255, 152, 0, 0.1));
            border: 1px solid rgba(255, 183, 77, 0.3);
            border-radius: 12px;
            padding: 16px;
            margin: 20px 0;
            cursor: pointer;
            transition: all 0.3s ease;
        }
        
        .academy-training:hover {
            background: linear-gradient(135deg, rgba(255, 183, 77, 0.15), rgba(255, 152, 0, 0.15));
            border-color: rgba(255, 183, 77, 0.5);
            transform: translateY(-2px);
        }
        
        .academy-training .header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 12px;
        }
        
        .academy-training .agent-name {
            font-weight: 600;
            color: #FFB74D;
            text-decoration: underline;
            font-size: 16px;
        }
        
        .academy-training .status {
            font-size: 12px;
            color: #8a92a5;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .progress-bar {
            width: 100%;
            height: 8px;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 4px;
            overflow: hidden;
            margin-bottom: 8px;
        }
        
        .progress-fill {
            height: 100%;
            background: linear-gradient(90deg, #FFB74D, #FF9800);
            border-radius: 4px;
            transition: width 0.5s ease;
        }
        
        .progress-text {
            font-size: 12px;
            color: #8a92a5;
        }
        
        /* Input Area */
        .input-area {
            padding: 20px 30px;
            border-top: 1px solid rgba(255, 255, 255, 0.1);
            background: rgba(20, 25, 35, 0.9);
            backdrop-filter: blur(10px);
        }
        
        .input-container {
            display: flex;
            gap: 12px;
            align-items: flex-end;
        }
        
        .input-field {
            flex: 1;
            background: rgba(255, 255, 255, 0.08);
            border: 1px solid rgba(255, 255, 255, 0.2);
            border-radius: 24px;
            padding: 14px 20px;
            color: #e0e6ed;
            font-size: 16px;
            resize: none;
            outline: none;
            transition: all 0.3s ease;
            min-height: 48px;
            max-height: 120px;
        }
        
        .input-field:focus {
            border-color: #4FC3F7;
            background: rgba(255, 255, 255, 0.12);
        }
        
        .input-field::placeholder {
            color: #8a92a5;
        }
        
        .send-button {
            width: 48px;
            height: 48px;
            background: linear-gradient(135deg, #4FC3F7, #29B6F6);
            border: none;
            border-radius: 50%;
            color: white;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            transition: all 0.3s ease;
            font-size: 18px;
        }
        
        .send-button:hover {
            transform: scale(1.05);
            background: linear-gradient(135deg, #29B6F6, #1976D2);
        }
        
        .send-button:disabled {
            background: rgba(255, 255, 255, 0.1);
            cursor: not-allowed;
            transform: none;
        }
        
        /* Agent Selector */
        .agent-selector {
            margin: 20px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 12px;
            padding: 15px;
        }
        
        .agent-selector h3 {
            font-size: 14px;
            color: #8a92a5;
            margin-bottom: 12px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .agent-list {
            display: flex;
            flex-direction: column;
            gap: 8px;
        }
        
        .agent-item {
            display: flex;
            align-items: center;
            padding: 10px 12px;
            border-radius: 8px;
            cursor: pointer;
            transition: all 0.3s ease;
            border: 1px solid transparent;
        }
        
        .agent-item:hover {
            background: rgba(255, 255, 255, 0.1);
        }
        
        .agent-item.selected {
            background: linear-gradient(135deg, rgba(79, 195, 247, 0.2), rgba(41, 182, 246, 0.2));
            border-color: rgba(79, 195, 247, 0.5);
        }
        
        .agent-item.active {
            background: linear-gradient(135deg, rgba(76, 175, 80, 0.2), rgba(139, 195, 74, 0.2));
            border-color: rgba(76, 175, 80, 0.5);
        }
        
        .agent-avatar {
            width: 32px;
            height: 32px;
            border-radius: 50%;
            background: linear-gradient(135deg, #4FC3F7, #29B6F6);
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 12px;
            font-size: 16px;
            position: relative;
        }
        
        .agent-status {
            position: absolute;
            bottom: -2px;
            right: -2px;
            width: 12px;
            height: 12px;
            border-radius: 50%;
            border: 2px solid rgba(20, 25, 35, 0.95);
        }
        
        .agent-status.online { background: #4CAF50; }
        .agent-status.busy { background: #FF9800; }
        .agent-status.training { background: #9C27B0; }
        
        .agent-info {
            flex: 1;
        }
        
        .agent-name {
            font-weight: 600;
            color: #e0e6ed;
            font-size: 14px;
        }
        
        .agent-role {
            font-size: 11px;
            color: #8a92a5;
            margin-top: 2px;
        }
        
        .multi-select {
            margin-top: 15px;
            padding-top: 15px;
            border-top: 1px solid rgba(255, 255, 255, 0.1);
        }
        
        .group-chat-btn {
            width: 100%;
            padding: 8px 12px;
            background: rgba(156, 39, 176, 0.1);
            border: 1px solid rgba(156, 39, 176, 0.3);
            border-radius: 8px;
            color: #BA68C8;
            font-size: 12px;
            cursor: pointer;
            transition: all 0.3s ease;
        }
        
        .group-chat-btn:hover {
            background: rgba(156, 39, 176, 0.2);
            border-color: rgba(156, 39, 176, 0.5);
        }
        
        /* Academy Actions */
        .academy-actions {
            margin-top: 20px;
            display: flex;
            gap: 10px;
            flex-wrap: wrap;
        }
        
        .academy-button {
            padding: 8px 16px;
            background: rgba(76, 175, 80, 0.1);
            border: 1px solid rgba(76, 175, 80, 0.3);
            border-radius: 20px;
            color: #4CAF50;
            text-decoration: none;
            font-size: 14px;
            cursor: pointer;
            transition: all 0.3s ease;
            display: inline-flex;
            align-items: center;
            gap: 6px;
        }
        
        .academy-button:hover {
            background: rgba(76, 175, 80, 0.2);
            border-color: rgba(76, 175, 80, 0.5);
            transform: translateY(-1px);
        }
        
        /* Cost Tracker */
        .cost-display {
            padding: 15px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 8px;
            margin-bottom: 20px;
        }
        
        .cost-display h3 {
            font-size: 14px;
            color: #8a92a5;
            margin-bottom: 10px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .cost-stats {
            display: flex;
            justify-content: space-between;
            font-size: 12px;
        }
        
        .cost-item {
            text-align: center;
        }
        
        .cost-value {
            font-size: 18px;
            font-weight: 600;
            color: #4FC3F7;
        }
        
        .cost-label {
            color: #8a92a5;
            margin-top: 4px;
        }
        
        /* Project Management */
        .project-manager {
            background: rgba(255,255,255,0.05);
            border-radius: 12px;
            padding: 15px;
            margin-bottom: 20px;
            border: 1px solid rgba(255,255,255,0.1);
        }
        
        .project-manager h3 {
            margin: 0 0 15px 0;
            color: #E0E0E0;
            font-size: 14px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .project-list {
            max-height: 150px;
            overflow-y: auto;
        }
        
        .project-card {
            background: rgba(255,255,255,0.08);
            border-radius: 8px;
            padding: 10px;
            margin-bottom: 6px;
            border: 1px solid rgba(255,255,255,0.1);
            transition: all 0.2s ease;
            cursor: pointer;
        }
        
        .project-card:hover {
            background: rgba(255,255,255,0.12);
            transform: translateY(-1px);
        }
        
        .project-card.active {
            border-color: #4FC3F7;
            background: rgba(79, 195, 247, 0.2);
        }
        
        .project-name {
            font-weight: bold;
            color: #4FC3F7;
            font-size: 13px;
            margin-bottom: 4px;
        }
        
        .project-path {
            font-size: 10px;
            color: #8a92a5;
            margin-bottom: 4px;
            opacity: 0.8;
        }
        
        .project-info {
            font-size: 9px;
            color: #8a92a5;
            display: flex;
            justify-content: space-between;
        }
        
        /* Persona Management */
        .persona-manager {
            background: rgba(255,255,255,0.05);
            border-radius: 12px;
            padding: 15px;
            margin-bottom: 20px;
            border: 1px solid rgba(255,255,255,0.1);
        }
        
        .persona-manager h3 {
            margin: 0 0 15px 0;
            color: #E0E0E0;
            font-size: 14px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .persona-list {
            max-height: 250px;
            overflow-y: auto;
            margin-bottom: 15px;
        }
        
        .persona-card {
            background: rgba(255,255,255,0.08);
            border-radius: 8px;
            padding: 12px;
            margin-bottom: 8px;
            border: 1px solid rgba(255,255,255,0.1);
            transition: all 0.2s ease;
        }
        
        .persona-card:hover {
            background: rgba(255,255,255,0.12);
            transform: translateY(-1px);
        }
        
        .persona-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 8px;
        }
        
        .persona-name {
            font-weight: bold;
            color: #FFB74D;
            font-size: 13px;
        }
        
        .persona-scope {
            font-size: 9px;
            padding: 2px 6px;
            border-radius: 10px;
            text-transform: uppercase;
            font-weight: bold;
            letter-spacing: 0.5px;
        }
        
        .persona-scope.project {
            background: rgba(76, 175, 80, 0.3);
            color: #4CAF50;
        }
        
        .persona-scope.user {
            background: rgba(33, 150, 243, 0.3);
            color: #2196F3;
        }
        
        .persona-scope.organization {
            background: rgba(156, 39, 176, 0.3);
            color: #9C27B0;
        }
        
        .persona-info {
            font-size: 11px;
            color: #8a92a5;
            margin-bottom: 8px;
        }
        
        .persona-specialization {
            color: #4FC3F7;
            font-weight: 500;
        }
        
        .persona-actions {
            display: flex;
            gap: 4px;
            flex-wrap: wrap;
        }
        
        .persona-action-btn {
            background: rgba(255,255,255,0.1);
            border: 1px solid rgba(255,255,255,0.2);
            color: #E0E0E0;
            padding: 3px 6px;
            border-radius: 4px;
            font-size: 9px;
            cursor: pointer;
            transition: all 0.2s ease;
            text-transform: uppercase;
            font-weight: bold;
            letter-spacing: 0.5px;
        }
        
        .persona-action-btn:hover {
            background: rgba(255,255,255,0.2);
            transform: translateY(-1px);
        }
        
        .persona-action-btn.deploy {
            background: rgba(76, 175, 80, 0.3);
            color: #4CAF50;
            border-color: #4CAF50;
        }
        
        .persona-action-btn.share {
            background: rgba(255, 193, 7, 0.3);
            color: #FFC107;
            border-color: #FFC107;
        }
        
        .persona-action-btn.delete {
            background: rgba(244, 67, 54, 0.3);
            color: #F44336;
            border-color: #F44336;
        }
        
        .persona-lora-info {
            font-size: 9px;
            color: #4FC3F7;
            margin-top: 4px;
            display: flex;
            gap: 8px;
        }
        
        .refresh-personas-btn {
            background: rgba(79, 195, 247, 0.2);
            border: 1px solid #4FC3F7;
            color: #4FC3F7;
            padding: 6px 10px;
            border-radius: 6px;
            font-size: 11px;
            cursor: pointer;
            transition: all 0.2s ease;
            width: 100%;
        }
        
        .refresh-personas-btn:hover {
            background: rgba(79, 195, 247, 0.3);
        }
        
        .loading {
            text-align: center;
            color: #8a92a5;
            font-size: 11px;
            padding: 20px;
        }
        
        /* Responsive */
        @media (max-width: 768px) {
            .app-container {
                flex-direction: column;
            }
            
            .sidebar {
                width: 100%;
                height: auto;
                order: 2;
            }
            
            .main-content {
                order: 1;
                height: 70vh;
            }
        }
    </style>
</head>
<body>
    <div class="app-container">
        <!-- Sidebar -->
        <div class="sidebar">
            <div class="sidebar-header">
                <div class="logo">
                    <div class="logo-icon">🧠</div>
                    <div class="logo-text">Continuum</div>
                </div>
                <div class="subtitle">AI Workforce Construction</div>
                <div class="status-pill">
                    <div class="status-dot"></div>
                    Academy Ready
                </div>
            </div>
            
            <!-- Room Tabs -->
            <div class="room-tabs">
                <div class="room-tab active" onclick="switchRoom('general')" id="tab-general">
                    💬 General
                </div>
                <div class="room-tab" onclick="switchRoom('academy')" id="tab-academy">
                    🎓 Academy
                </div>
            </div>
            
            <!-- Cost Tracker -->
            <div class="cost-display">
                <h3>Session Costs</h3>
                <div class="cost-stats">
                    <div class="cost-item">
                        <div class="cost-value" id="cost-requests">${this.continuum.costTracker ? this.continuum.costTracker.getRequests() : 0}</div>
                        <div class="cost-label">Requests</div>
                    </div>
                    <div class="cost-item">
                        <div class="cost-value" id="cost-total">$${this.continuum.costTracker ? this.continuum.costTracker.getTotal().toFixed(4) : '0.0000'}</div>
                        <div class="cost-label">Cost</div>
                    </div>
                </div>
            </div>
            
            <!-- Project Management -->
            <div class="project-manager">
                <h3>Active Projects</h3>
                <div class="project-list" id="project-list">
                    <div class="loading">Loading projects...</div>
                </div>
            </div>
            
            <!-- Persona Management -->
            <div class="persona-manager">
                <h3>Saved Personas</h3>
                <div class="persona-list" id="persona-list">
                    <div class="loading">Loading personas...</div>
                </div>
                <button class="refresh-personas-btn" onclick="refreshPersonas()">
                    🔄 Refresh
                </button>
            </div>
            
            <!-- Room-specific Content -->
            <div id="room-content">
                <!-- General Room Content -->
                <div id="general-room" class="room-content">
                    <!-- Agent Selector -->
                    <div class="agent-selector">
                        <h3>Available Agents</h3>
                        <div class="agent-list">
                            <div class="agent-item selected" onclick="selectAgent('auto')" id="agent-auto">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #4FC3F7, #29B6F6);">
                                    🧠
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info">
                                    <div class="agent-name">Auto Route</div>
                                    <div class="agent-role">Smart agent selection</div>
                                </div>
                            </div>
                            
                            <div class="agent-item" onclick="selectAgent('PlannerAI')" id="agent-PlannerAI">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #9C27B0, #673AB7);">
                                    📋
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info">
                                    <div class="agent-name">PlannerAI</div>
                                    <div class="agent-role">Strategy & web commands</div>
                                </div>
                            </div>
                            
                            <div class="agent-item" onclick="selectAgent('CodeAI')" id="agent-CodeAI">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #FF5722, #F44336);">
                                    💻
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info">
                                    <div class="agent-name">CodeAI</div>
                                    <div class="agent-role">Code analysis & debugging</div>
                                </div>
                            </div>
                            
                            <div class="agent-item" onclick="selectAgent('GeneralAI')" id="agent-GeneralAI">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #4CAF50, #8BC34A);">
                                    💬
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info">
                                    <div class="agent-name">GeneralAI</div>
                                    <div class="agent-role">General assistance</div>
                                </div>
                            </div>
                            
                            <div class="agent-item" onclick="selectAgent('ProtocolSheriff')" id="agent-ProtocolSheriff">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #FF9800, #FFC107);">
                                    🛡️
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info">
                                    <div class="agent-name">Protocol Sheriff</div>
                                    <div class="agent-role">Response validation</div>
                                </div>
                            </div>
                        </div>
                        
                        <div class="multi-select">
                            <button class="group-chat-btn" onclick="startGroupChat()">
                                👥 Start Group Chat
                            </button>
                        </div>
                    </div>
                </div>
                
                <!-- Academy Room Content -->
                <div id="academy-room" class="room-content" style="display: none;">
                    <div id="academy-section">
                        ${this.academyInterface.generateAcademyHTML()}
                    </div>
                    
                    <div class="academy-actions">
                        <button class="academy-button" onclick="sendSheriffToAcademy()">
                            🛡️ Send Sheriff to Academy
                        </button>
                        <button class="academy-button" onclick="trainCustomPersona()">
                            🎓 Train Custom Persona
                        </button>
                    </div>
                </div>
            </div>
        </div>
        
        <!-- Main Chat -->
        <div class="main-content">
            <div class="chat-header">
                <div class="chat-title" id="chat-title">General Chat</div>
                <div class="chat-subtitle" id="chat-subtitle">Talk to specialized AI agents</div>
            </div>
            
            <div class="chat-container" id="chat"></div>
            
            <div class="input-area">
                <div class="input-container">
                    <textarea 
                        id="messageInput" 
                        class="input-field" 
                        placeholder="Ask anything - the AI will coordinate automatically..."
                        rows="1"
                        onkeydown="handleKeyDown(event)"
                    ></textarea>
                    <button class="send-button" id="sendButton" onclick="sendMessage()">
                        ➤
                    </button>
                </div>
            </div>
        </div>
    </div>

    <script>
        let ws;
        let messageHistory = [];
        let currentRoom = 'general';
        let roomMessages = {
            general: [],
            academy: []
        };
        let selectedAgent = 'auto';
        let selectedAgents = new Set();
        let isGroupChat = false;
        
        // Initialize WebSocket connection
        function initWebSocket() {
            ws = new WebSocket('ws://localhost:${this.continuum.port}');
            
            ws.onopen = function() {
                console.log('Connected to Continuum');
                addSystemMessage('🟢 Connected to Academy-trained Claude instances');
            };
            
            ws.onmessage = function(event) {
                const data = JSON.parse(event.data);
                handleWebSocketMessage(data);
            };
            
            ws.onclose = function() {
                console.log('Disconnected from Continuum');
                addSystemMessage('🔴 Disconnected from Academy');
                setTimeout(initWebSocket, 3000); // Reconnect
            };
            
            ws.onerror = function(error) {
                console.error('WebSocket error:', error);
                addSystemMessage('⚠️ Connection error');
            };
        }
        
        function handleWebSocketMessage(data) {
            if (data.type === 'response') {
                addMessage(data.agent || 'AI Agent', data.message, 'ai');
                updateCosts(data.cost, data.requests);
            } else if (data.type === 'academy_update') {
                handleAcademyUpdate(data);
            } else if (data.type === 'academy_status') {
                updateAcademySection(data);
            }
        }
        
        function handleAcademyUpdate(data) {
            const { personaName, session } = data;
            
            // Show academy training status in Academy room
            if (session.status === 'enrolling') {
                addMessage('Academy System', \`🎓 \${personaName} is now training in the Academy\`, 'ai', true);
            } else if (session.status === 'graduated') {
                addMessage('Academy System', \`🎉 \${personaName} graduated from Academy! Ready for deployment.\`, 'ai', true);
            } else if (session.status === 'failed') {
                addMessage('Academy System', \`❌ \${personaName} failed Academy training. Can be re-enrolled.\`, 'ai', true);
            }
            
            // Update academy section
            updateAcademyDisplay();
        }
        
        // Room switching functionality
        function switchRoom(room) {
            currentRoom = room;
            
            // Update tab appearance
            document.querySelectorAll('.room-tab').forEach(tab => tab.classList.remove('active'));
            document.getElementById(\`tab-\${room}\`).classList.add('active');
            
            // Update sidebar content
            document.querySelectorAll('.room-content').forEach(content => content.style.display = 'none');
            document.getElementById(\`\${room}-room\`).style.display = 'block';
            
            // Update chat header
            if (room === 'general') {
                document.getElementById('chat-title').textContent = 'General Chat';
                document.getElementById('chat-subtitle').textContent = 'Talk to specialized AI agents';
            } else if (room === 'academy') {
                document.getElementById('chat-title').textContent = 'Academy Training Room';
                document.getElementById('chat-subtitle').textContent = 'Watch AI agents train and improve their skills';
            }
            
            // Clear and reload room messages
            const chat = document.getElementById('chat');
            chat.innerHTML = '';
            
            if (roomMessages[room]) {
                roomMessages[room].forEach(msg => {
                    addMessageToChat(msg.sender, msg.content, msg.type, msg.isAcademy);
                });
            }
            
            // Add welcome message for new rooms
            if (roomMessages[room].length === 0) {
                if (room === 'general') {
                    addMessage('System', '💬 Welcome to General Chat! Ask me anything and I\\'ll route it to the best AI agent.', 'ai');
                } else if (room === 'academy') {
                    addMessage('System', '🎓 Welcome to the Academy! Here you can watch AI agents train and see their progress in real-time.', 'ai');
                    updateAcademyDisplay();
                }
            }
        }
        
        function addMessage(sender, content, type = 'ai', isAcademy = false) {
            const messageObj = { sender, content, type, time: new Date(), isAcademy };
            
            // Add to appropriate room
            const targetRoom = isAcademy ? 'academy' : currentRoom;
            if (!roomMessages[targetRoom]) roomMessages[targetRoom] = [];
            roomMessages[targetRoom].push(messageObj);
            
            // Only show if we're in the target room
            if (currentRoom === targetRoom) {
                addMessageToChat(sender, content, type, isAcademy);
            }
            
            // Update message history for API
            messageHistory.push(messageObj);
        }
        
        function addMessageToChat(sender, content, type, isAcademy = false) {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = \`message \${type}\`;
            
            const now = new Date();
            const timeStr = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            
            messageDiv.innerHTML = \`
                <div class="message-sender">\${sender}</div>
                <div class="message-bubble">\${content}</div>
                <div class="message-time">\${timeStr}</div>
            \`;
            
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        function addAcademyMessage(content, session) {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'academy-training';
            messageDiv.onclick = () => toggleAcademyDetails(session.personaName);
            
            messageDiv.innerHTML = \`
                <div class="header">
                    <div class="agent-name">\${session.personaName}</div>
                    <div class="status">\${session.status.replace(/_/g, ' ')}</div>
                </div>
                <div>\${content}</div>
                \${session.progress !== undefined ? \`
                    <div class="progress-bar">
                        <div class="progress-fill" style="width: \${session.progress}%"></div>
                    </div>
                    <div class="progress-text">Progress: \${session.progress}% • Round \${session.currentRound}/\${session.totalRounds}</div>
                \` : ''}
            \`;
            
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        function addSystemMessage(content) {
            addMessage('System', content, 'ai');
        }
        
        // Agent selection functions
        function selectAgent(agentId) {
            if (isGroupChat) {
                // Multi-select mode for group chat
                if (selectedAgents.has(agentId)) {
                    selectedAgents.delete(agentId);
                    document.getElementById(\`agent-\${agentId}\`).classList.remove('selected');
                } else {
                    selectedAgents.add(agentId);
                    document.getElementById(\`agent-\${agentId}\`).classList.add('selected');
                }
                updateGroupChatStatus();
            } else {
                // Single select mode
                selectedAgent = agentId;
                document.querySelectorAll('.agent-item').forEach(item => item.classList.remove('selected'));
                document.getElementById(\`agent-\${agentId}\`).classList.add('selected');
                updateChatHeader();
            }
        }
        
        function startGroupChat() {
            isGroupChat = !isGroupChat;
            const button = document.querySelector('.group-chat-btn');
            
            if (isGroupChat) {
                button.textContent = '👤 Switch to Single Chat';
                button.style.background = 'rgba(255, 152, 0, 0.1)';
                button.style.borderColor = 'rgba(255, 152, 0, 0.3)';
                button.style.color = '#FFB74D';
                
                // Clear single selection and allow multi-select
                document.querySelectorAll('.agent-item').forEach(item => item.classList.remove('selected'));
                selectedAgents.clear();
                addMessage('System', '👥 Group chat mode enabled. Click multiple agents to include them in the conversation.', 'ai');
            } else {
                button.textContent = '👥 Start Group Chat';
                button.style.background = 'rgba(156, 39, 176, 0.1)';
                button.style.borderColor = 'rgba(156, 39, 176, 0.3)';
                button.style.color = '#BA68C8';
                
                // Return to single agent mode
                selectedAgents.clear();
                selectedAgent = 'auto';
                selectAgent('auto');
                addMessage('System', '👤 Single chat mode enabled. Select one agent to talk to.', 'ai');
            }
            
            updateChatHeader();
        }
        
        function updateChatHeader() {
            const title = document.getElementById('chat-title');
            const subtitle = document.getElementById('chat-subtitle');
            
            if (currentRoom === 'general') {
                if (isGroupChat && selectedAgents.size > 0) {
                    title.textContent = \`Group Chat (\${selectedAgents.size} agents)\`;
                    subtitle.textContent = \`Talking to: \${Array.from(selectedAgents).join(', ')}\`;
                } else if (selectedAgent !== 'auto') {
                    title.textContent = \`Chat with \${selectedAgent}\`;
                    subtitle.textContent = \`Direct conversation with \${selectedAgent}\`;
                } else {
                    title.textContent = 'General Chat';
                    subtitle.textContent = 'Smart agent routing with Protocol Sheriff validation';
                }
            }
        }
        
        function updateGroupChatStatus() {
            const button = document.querySelector('.group-chat-btn');
            if (isGroupChat) {
                button.textContent = \`👥 Group Chat (\${selectedAgents.size} selected)\`;
            }
        }
        
        function sendMessage() {
            const input = document.getElementById('messageInput');
            const message = input.value.trim();
            
            if (!message || !ws || ws.readyState !== WebSocket.OPEN) return;
            
            // Add message to current room
            addMessage('You', message, 'user');
            
            // Send to appropriate endpoint based on room and agent selection
            if (currentRoom === 'academy') {
                // Academy messages go to Academy system
                ws.send(JSON.stringify({
                    type: 'academy_message',
                    content: message,
                    room: 'academy'
                }));
            } else {
                // General messages - handle agent selection
                if (isGroupChat && selectedAgents.size > 0) {
                    // Group chat: send to multiple agents
                    ws.send(JSON.stringify({
                        type: 'group_message',
                        content: message,
                        agents: Array.from(selectedAgents),
                        room: 'general',
                        history: messageHistory.slice(-5)
                    }));
                } else if (selectedAgent !== 'auto') {
                    // Direct to specific agent (with Sheriff validation)
                    ws.send(JSON.stringify({
                        type: 'direct_message',
                        content: message,
                        agent: selectedAgent,
                        room: 'general',
                        history: messageHistory.slice(-5)
                    }));
                } else {
                    // Auto routing (with Sheriff validation)
                    ws.send(JSON.stringify({
                        type: 'message',
                        content: message,
                        room: 'general',
                        history: messageHistory.slice(-5)
                    }));
                }
            }
            
            input.value = '';
            autoResize(input);
        }
        
        function handleKeyDown(event) {
            if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                sendMessage();
            }
        }
        
        function autoResize(textarea) {
            textarea.style.height = 'auto';
            textarea.style.height = Math.min(textarea.scrollHeight, 120) + 'px';
        }
        
        function updateCosts(cost, requests) {
            document.getElementById('cost-requests').textContent = requests || 0;
            document.getElementById('cost-total').textContent = '$' + (cost || 0).toFixed(4);
        }
        
        // Persona Management Functions
        async function refreshPersonas() {
            const personaList = document.getElementById('persona-list');
            personaList.innerHTML = '<div class="loading">Loading personas...</div>';
            
            try {
                const response = await fetch('/api/personas');
                const personas = await response.json();
                
                if (personas.length === 0) {
                    personaList.innerHTML = '<div class="loading">No personas found</div>';
                    return;
                }
                
                personaList.innerHTML = '';
                personas.forEach(persona => {
                    const personaCard = createPersonaCard(persona);
                    personaList.appendChild(personaCard);
                });
                
            } catch (error) {
                console.error('Failed to load personas:', error);
                personaList.innerHTML = '<div class="loading" style="color: #F44336;">Failed to load personas</div>';
            }
        }
        
        function createPersonaCard(persona) {
            const card = document.createElement('div');
            card.className = 'persona-card';
            
            const scope = persona.scope || 'unknown';
            const hasAdapter = persona.hasLoRAAdapter;
            
            card.innerHTML = \`
                <div class="persona-header">
                    <div class="persona-name">\${persona.name}</div>
                    <div class="persona-scope \${scope}">\${scope}</div>
                </div>
                <div class="persona-info">
                    <div class="persona-specialization">\${persona.specialization}</div>
                    <div>Status: \${persona.status} | Score: \${persona.graduationScore}%</div>
                    \${hasAdapter ? \`<div class="persona-lora-info">
                        <span>📦 \${persona.adapterSize}</span>
                        <span>📊 \${persona.reductionFactor}</span>
                    </div>\` : ''}
                </div>
                <div class="persona-actions">
                    <button class="persona-action-btn deploy" onclick="deployPersona('\${persona.id}')">
                        🚀 Deploy
                    </button>
                    \${scope !== 'organization' ? \`
                        <button class="persona-action-btn share" onclick="sharePersona('\${persona.id}', 'organization')">
                            🤝 Share Org
                        </button>
                    \` : ''}
                    \${scope === 'user' ? \`
                        <button class="persona-action-btn share" onclick="sharePersona('\${persona.id}', 'project')">
                            🏢 Share Project
                        </button>
                    \` : ''}
                    <button class="persona-action-btn delete" onclick="deletePersona('\${persona.id}')">
                        🗑️ Delete
                    </button>
                </div>
            \`;
            
            return card;
        }
        
        async function sharePersona(personaId, toScope) {
            try {
                const response = await fetch(\`/api/personas/\${personaId}/share\`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ toScope })
                });
                
                const result = await response.json();
                
                if (result.success) {
                    addSystemMessage(\`🤝 Persona shared to \${toScope}: \${result.personaName}\`);
                    await refreshPersonas(); // Refresh the list
                } else {
                    addSystemMessage(\`❌ Failed to share persona: \${result.error}\`);
                }
                
            } catch (error) {
                console.error('Share persona error:', error);
                addSystemMessage(\`❌ Failed to share persona: \${error.message}\`);
            }
        }
        
        async function deployPersona(personaId) {
            try {
                const response = await fetch(\`/api/personas/\${personaId}/deploy\`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ task: 'Interactive session' })
                });
                
                const result = await response.json();
                
                if (result.success) {
                    const deployment = result.deployment;
                    const adapterInfo = deployment.deployment.adapterType === 'lora' ? 
                        \` with \${deployment.deployment.appliedAdapters?.length || 0} LoRA layers\` : '';
                    
                    addSystemMessage(\`🚀 Deployed: \${deployment.persona.name}\${adapterInfo}\`);
                    
                    // Switch to the deployed persona context
                    window.currentPersona = deployment.persona;
                    
                } else {
                    addSystemMessage(\`❌ Failed to deploy persona: \${result.error}\`);
                }
                
            } catch (error) {
                console.error('Deploy persona error:', error);
                addSystemMessage(\`❌ Failed to deploy persona: \${error.message}\`);
            }
        }
        
        async function deletePersona(personaId) {
            if (!confirm('Are you sure you want to delete this persona? This action cannot be undone.')) {
                return;
            }
            
            try {
                const response = await fetch(\`/api/personas/\${personaId}\`, {
                    method: 'DELETE'
                });
                
                const result = await response.json();
                
                if (result.success) {
                    addSystemMessage(\`🗑️ Persona deleted: \${result.personaName}\`);
                    await refreshPersonas(); // Refresh the list
                } else {
                    addSystemMessage(\`❌ Failed to delete persona: \${result.error}\`);
                }
                
            } catch (error) {
                console.error('Delete persona error:', error);
                addSystemMessage(\`❌ Failed to delete persona: \${error.message}\`);
            }
        }
        
        // Project Management Functions
        async function refreshProjects() {
            const projectList = document.getElementById('project-list');
            projectList.innerHTML = '<div class="loading">Loading projects...</div>';
            
            try {
                const response = await fetch('/api/projects');
                const projects = await response.json();
                
                if (projects.length === 0) {
                    projectList.innerHTML = '<div class="loading">No projects registered</div>';
                    return;
                }
                
                projectList.innerHTML = '';
                projects.forEach(project => {
                    const projectCard = createProjectCard(project);
                    projectList.appendChild(projectCard);
                });
                
            } catch (error) {
                console.error('Failed to load projects:', error);
                projectList.innerHTML = '<div class="loading" style="color: #F44336;">Failed to load projects</div>';
            }
        }
        
        function createProjectCard(project) {
            const card = document.createElement('div');
            card.className = 'project-card';
            
            // Determine if this is the current project
            const isCurrentProject = window.location.pathname === '/' && 
                project.workingDirectory === '${process.cwd()}';
            
            if (isCurrentProject) {
                card.classList.add('active');
            }
            
            card.innerHTML = \`
                <div class="project-name">\${project.projectName}</div>
                <div class="project-path">\${project.workingDirectory}</div>
                <div class="project-info">
                    <span>PID: \${project.pid}</span>
                    <span>\${new Date(project.registeredAt).toLocaleTimeString()}</span>
                </div>
            \`;
            
            card.onclick = () => switchToProject(project);
            
            return card;
        }
        
        function switchToProject(project) {
            // Update the active project indicator
            document.querySelectorAll('.project-card').forEach(card => {
                card.classList.remove('active');
            });
            event.target.closest('.project-card').classList.add('active');
            
            // Update the AI context to use this project's scratchpad
            window.currentProject = project;
            addSystemMessage(\`📁 Switched to project: \${project.projectName}\`);
            addSystemMessage(\`💾 Using scratchpad: \${project.scratchpadPath}\`);
        }
        
        // Academy functions
        function sendSheriffToAcademy() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'start_academy_training',
                    personaName: 'Sheriff-' + Date.now(),
                    specialization: 'protocol_enforcement',
                    rounds: 10
                }));
                addSystemMessage('🛡️ Sheriff sent to Academy for training...');
            }
        }
        
        function trainCustomPersona() {
            const personaName = prompt('Enter persona name:');
            const specialization = prompt('Enter specialization:') || 'protocol_enforcement';
            
            if (personaName && ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'start_academy_training',
                    personaName: personaName,
                    specialization: specialization,
                    rounds: 10
                }));
                addSystemMessage(\`🎓 \${personaName} sent to Academy for \${specialization} training...\`);
            }
        }
        
        function updateAcademyDisplay() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'get_academy_status' }));
            }
        }
        
        function toggleAcademyDetails(personaName) {
            console.log('Toggle details for:', personaName);
        }
        
        // Auto-resize textarea
        document.getElementById('messageInput').addEventListener('input', function() {
            autoResize(this);
        });
        
        // Initialize
        initWebSocket();
        
        // Initialize default room
        document.addEventListener('DOMContentLoaded', function() {
            switchRoom('general');
            refreshProjects(); // Load active projects
            refreshPersonas(); // Load personas on startup
        });
        
        // If page is already loaded
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', function() {
                switchRoom('general');
                updateChatHeader();
                refreshProjects(); // Load active projects
                refreshPersonas(); // Load personas on startup
            });
        } else {
            switchRoom('general');
            updateChatHeader();
            refreshProjects(); // Load active projects
            refreshPersonas(); // Load personas on startup
        }
        
        // Load conversation history
        ${this.generateConversationHistory()}
        
        // Academy JavaScript
        ${this.academyInterface.generateAcademyJS()}
    </script>
</body>
</html>`;
  }

  generateConversationHistory() {
    return `
        // Load any existing conversation history here
        // This could come from a database or session storage
    `;
  }
}

module.exports = UIGenerator;