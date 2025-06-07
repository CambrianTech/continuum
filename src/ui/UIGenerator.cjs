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
    
    // Initialize progressive web components enhancement
    const WebComponentsIntegration = require('./WebComponentsIntegration.cjs');
    this.webComponentsIntegration = new WebComponentsIntegration();
  }

  generateHTML() {
    // Clear require cache to get fresh version info
    delete require.cache[require.resolve('../../package.json')];
    const packageInfo = require('../../package.json');
    
    const html = `<!DOCTYPE html>
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
            display: flex;
            align-items: flex-start;
            justify-content: space-between;
        }
        
        .chat-header-content {
            flex: 1;
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
        
        .clear-chat-btn {
            background: rgba(244, 67, 54, 0.2);
            border: 1px solid #F44336;
            color: #F44336;
            border-radius: 6px;
            padding: 8px 12px;
            font-size: 12px;
            cursor: pointer;
            transition: all 0.2s ease;
            display: flex;
            align-items: center;
            gap: 5px;
            white-space: nowrap;
        }
        
        .clear-chat-btn:hover {
            background: rgba(244, 67, 54, 0.3);
            transform: translateY(-1px);
        }
        
        .connection-status {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            display: inline-block;
            margin: 0;
            transition: all 0.5s ease;
            animation: hal-pulse 3s infinite;
            vertical-align: middle;
            position: relative;
            flex-shrink: 0;
        }
        
        .connection-status.connected {
            background: radial-gradient(circle, #4CAF50 0%, #2E7D32 40%, rgba(76, 175, 80, 0.3) 70%, transparent 100%);
            box-shadow: 
                0 0 6px rgba(76, 175, 80, 0.8),
                0 0 12px rgba(76, 175, 80, 0.6),
                0 0 18px rgba(76, 175, 80, 0.4),
                0 0 24px rgba(76, 175, 80, 0.2),
                inset 0 0 6px rgba(255, 255, 255, 0.2);
        }
        
        .connection-status.disconnected {
            background: radial-gradient(circle, #F44336 0%, #C62828 40%, rgba(244, 67, 54, 0.3) 70%, transparent 100%);
            box-shadow: 
                0 0 6px rgba(244, 67, 54, 0.8),
                0 0 12px rgba(244, 67, 54, 0.6),
                0 0 18px rgba(244, 67, 54, 0.4),
                0 0 24px rgba(244, 67, 54, 0.2),
                inset 0 0 6px rgba(255, 255, 255, 0.2);
            animation: hal-pulse-error 2s infinite;
        }
        
        .connection-status.connecting {
            background: radial-gradient(circle, #FF9800 0%, #F57C00 40%, rgba(255, 152, 0, 0.3) 70%, transparent 100%);
            box-shadow: 
                0 0 6px rgba(255, 152, 0, 0.8),
                0 0 12px rgba(255, 152, 0, 0.6),
                0 0 18px rgba(255, 152, 0, 0.4),
                0 0 24px rgba(255, 152, 0, 0.2),
                inset 0 0 6px rgba(255, 255, 255, 0.2);
            animation: hal-pulse-warning 1.5s infinite;
        }
        
        @keyframes hal-pulse {
            0% { 
                transform: scale(1); 
                opacity: 0.9;
                filter: blur(0px);
            }
            50% { 
                transform: scale(1.05); 
                opacity: 1;
                filter: blur(0.5px);
            }
            100% { 
                transform: scale(1); 
                opacity: 0.9;
                filter: blur(0px);
            }
        }
        
        @keyframes hal-pulse-error {
            0% { 
                transform: scale(1); 
                opacity: 0.8;
                filter: blur(0px);
            }
            25% { 
                transform: scale(1.1); 
                opacity: 1;
                filter: blur(0.8px);
            }
            50% { 
                transform: scale(0.95); 
                opacity: 0.7;
                filter: blur(0.3px);
            }
            75% { 
                transform: scale(1.1); 
                opacity: 1;
                filter: blur(0.8px);
            }
            100% { 
                transform: scale(1); 
                opacity: 0.8;
                filter: blur(0px);
            }
        }
        
        @keyframes hal-pulse-warning {
            0% { 
                transform: scale(1); 
                opacity: 0.85;
                filter: blur(0px);
            }
            33% { 
                transform: scale(1.08); 
                opacity: 1;
                filter: blur(0.6px);
            }
            66% { 
                transform: scale(0.98); 
                opacity: 0.75;
                filter: blur(0.2px);
            }
            100% { 
                transform: scale(1); 
                opacity: 0.85;
                filter: blur(0px);
            }
        }

        /* AI Cursor Mode - When HAL becomes the mouse */
        .connection-status.ai-cursor {
            position: fixed !important;
            z-index: 10000 !important;
            pointer-events: none;
            transform: none !important;
            transition: left 0.3s cubic-bezier(0.25, 0.46, 0.45, 0.94), 
                       top 0.3s cubic-bezier(0.25, 0.46, 0.45, 0.94);
            background: radial-gradient(circle, #00ff41, #00cc33) !important;
            box-shadow: 0 0 20px rgba(0, 255, 65, 1), 
                       0 0 40px rgba(0, 255, 65, 0.8),
                       0 0 60px rgba(0, 255, 65, 0.6) !important;
            animation: ai-cursor-pulse 1s infinite !important;
            width: 16px !important;
            height: 16px !important;
        }

        @keyframes ai-cursor-pulse {
            0%, 100% { 
                transform: scale(1);
                background: radial-gradient(circle, #00ff41, #00cc33);
                box-shadow: 0 0 20px rgba(0, 255, 65, 1), 
                           0 0 40px rgba(0, 255, 65, 0.8),
                           0 0 60px rgba(0, 255, 65, 0.6);
            }
            50% { 
                transform: scale(1.2);
                background: radial-gradient(circle, #00ff88, #00ff41);
                box-shadow: 0 0 30px rgba(0, 255, 65, 1), 
                           0 0 60px rgba(0, 255, 65, 0.9),
                           0 0 90px rgba(0, 255, 65, 0.7);
            }
        }

        /* Click feedback animation */
        .connection-status.ai-cursor-click {
            animation: ai-cursor-click 0.3s ease-out !important;
        }

        @keyframes ai-cursor-click {
            0% { 
                transform: scale(1);
                background: radial-gradient(circle, #00ff41, #00cc33);
            }
            50% { 
                transform: scale(2);
                background: radial-gradient(circle, #ffffff, #00ff41);
                box-shadow: 0 0 50px rgba(255, 255, 255, 1), 
                           0 0 100px rgba(0, 255, 65, 1);
            }
            100% { 
                transform: scale(1);
                background: radial-gradient(circle, #00ff41, #00cc33);
            }
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
            max-height: 400px;
            overflow-y: auto;
            display: flex;
            flex-direction: column;
        }
        
        .agent-selector h3 {
            font-size: 14px;
            color: #8a92a5;
            margin-bottom: 12px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            flex-shrink: 0;
        }
        
        .agent-list {
            display: flex;
            flex-direction: column;
            gap: 6px;
            flex: 1;
            overflow-y: auto;
            padding-right: 5px;
        }
        
        .agent-list::-webkit-scrollbar {
            width: 6px;
        }
        
        .agent-list::-webkit-scrollbar-track {
            background: rgba(255, 255, 255, 0.05);
            border-radius: 3px;
        }
        
        .agent-list::-webkit-scrollbar-thumb {
            background: rgba(255, 255, 255, 0.2);
            border-radius: 3px;
        }
        
        .agent-list::-webkit-scrollbar-thumb:hover {
            background: rgba(255, 255, 255, 0.3);
        }
        
        .agent-item {
            display: flex;
            align-items: center;
            padding: 8px 10px;
            border-radius: 8px;
            cursor: pointer;
            transition: all 0.3s ease;
            border: 1px solid transparent;
            flex-shrink: 0;
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
            width: 30px;
            height: 30px;
            border-radius: 50%;
            background: linear-gradient(135deg, #4FC3F7, #29B6F6);
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 10px;
            font-size: 14px;
            position: relative;
            flex-shrink: 0;
        }
        
        .agent-status {
            position: absolute;
            bottom: -1px;
            right: -1px;
            width: 10px;
            height: 10px;
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
        
        .persona-action-btn.retrain {
            background: rgba(255, 152, 0, 0.3);
            color: #FF9800;
            border-color: #FF9800;
        }
        
        .agent-dropdown-btn {
            background: transparent;
            border: none;
            color: rgba(0, 212, 255, 0.4);
            font-size: 14px;
            cursor: pointer;
            padding: 4px;
            margin-left: 8px;
            padding: 2px;
            margin-left: 6px;
            width: 20px;
            height: 20px;
            display: inline-flex;
            align-items: center;
            justify-content: center;
            position: relative;
            transition: all 0.3s ease;
            clip-path: polygon(30% 0%, 70% 0%, 100% 30%, 100% 70%, 70% 100%, 30% 100%, 0% 70%, 0% 30%);
        }
        
        .agent-dropdown-btn::before {
            content: '';
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: linear-gradient(45deg, transparent, rgba(0, 212, 255, 0.1), transparent);
            clip-path: inherit;
            transition: all 0.3s ease;
            opacity: 0;
        }
        
        .agent-dropdown-btn::after {
            content: '⬢';
            font-size: 16px;
            color: rgba(0, 212, 255, 0.3);
            transition: all 0.3s ease;
        }
        
        .agent-dropdown-btn:hover::before {
            opacity: 1;
            background: linear-gradient(45deg, transparent, rgba(0, 212, 255, 0.3), transparent);
        }
        
        .agent-dropdown-btn:hover::after {
            color: rgba(0, 212, 255, 0.8);
            text-shadow: 0 0 8px rgba(0, 212, 255, 0.6);
        }
        
        .agent-dropdown-btn.active::before {
            opacity: 1;
            background: linear-gradient(45deg, rgba(0, 212, 255, 0.2), rgba(0, 212, 255, 0.4), rgba(0, 212, 255, 0.2));
            animation: pulse-hex 2s infinite ease-in-out;
        }
        
        .agent-dropdown-btn.active::after {
            color: rgba(0, 212, 255, 1);
            text-shadow: 0 0 12px rgba(0, 212, 255, 0.8);
        }
        
        @keyframes pulse-hex {
            0%, 100% { opacity: 0.8; }
            50% { opacity: 1; }
        }
        
        .agent-info-dropdown {
            margin-top: 10px;
            padding: 12px;
            background: linear-gradient(135deg, #1a1a2e 0%, #2a2a3e 100%);
            border: 1px solid #333;
            border-radius: 8px;
            box-shadow: 0 4px 15px rgba(0, 0, 0, 0.4);
            animation: slideDown 0.3s ease;
        }
        
        .agent-stats div {
            margin: 6px 0;
            font-size: 11px;
            color: #ccc;
            line-height: 1.4;
        }
        
        .agent-action-btn {
            background: rgba(255, 152, 0, 0.3);
            color: #FF9800;
            border: 1px solid #FF9800;
            padding: 4px 8px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 10px;
            transition: all 0.2s ease;
            margin-top: 5px;
        }
        
        .agent-action-btn:hover {
            background: rgba(255, 152, 0, 0.5);
            transform: translateY(-1px);
        }
        
        .agent-actions {
            margin-top: 8px;
            text-align: right;
        }
        
        /* Cyberpunk Glass Details Panel */
        .agent-details-panel {
            position: fixed;
            bottom: 0;
            left: -350px;
            width: 300px;
            height: calc(100vh - 200px);
            max-height: calc(100vh - 200px);
            background: 
                linear-gradient(145deg, 
                    rgba(0, 212, 255, 0.03), 
                    rgba(20, 25, 35, 0.95),
                    rgba(0, 150, 255, 0.03)
                ),
                radial-gradient(circle at 20% 80%, rgba(0, 212, 255, 0.05) 0%, transparent 50%),
                linear-gradient(45deg, transparent 48%, rgba(0, 212, 255, 0.08) 49%, rgba(0, 212, 255, 0.08) 51%, transparent 52%);
            background-size: 100% 100%, 100% 100%, 30px 30px;
            backdrop-filter: blur(20px) saturate(180%);
            -webkit-backdrop-filter: blur(20px) saturate(180%);
            clip-path: polygon(0 0, 80% 0, 90% 10%, 100% 20%, 100% 80%, 90% 90%, 80% 100%, 0 100%);
            border: 1px solid rgba(0, 212, 255, 0.25);
            border-left: 2px solid rgba(0, 212, 255, 0.4);
            box-shadow: 
                0 0 40px rgba(0, 0, 0, 0.8),
                inset 0 0 40px rgba(0, 212, 255, 0.05),
                inset 1px 0 1px rgba(0, 212, 255, 0.2);
            z-index: 150;
            transition: left 0.5s cubic-bezier(0.23, 1, 0.32, 1);
            overflow-y: auto;
            display: flex;
            flex-direction: column;
        }
        
        .agent-details-panel::before {
            content: '';
            position: absolute;
            top: 20px;
            right: 20px;
            width: 40px;
            height: 1px;
            background: linear-gradient(90deg, transparent, rgba(0, 212, 255, 0.6), transparent);
            opacity: 0.7;
        }
        
        .agent-details-panel::after {
            content: '';
            position: absolute;
            bottom: 20px;
            left: 20px;
            width: 60px;
            height: 1px;
            background: linear-gradient(90deg, rgba(0, 212, 255, 0.6), transparent);
            opacity: 0.7;
        }
        
        .agent-details-panel.open {
            left: 300px;
        }
        
        .agent-details-panel.fast-close {
            transition: left 0.12s ease-in;
        }
        
        .agent-details-header {
            padding: 12px 18px;
            border-bottom: 1px solid rgba(0, 212, 255, 0.15);
            background: 
                linear-gradient(135deg, 
                    rgba(0, 212, 255, 0.08), 
                    rgba(20, 25, 35, 0.9)
                ),
                linear-gradient(90deg, transparent 0%, rgba(0, 212, 255, 0.05) 50%, transparent 100%);
            display: flex;
            align-items: center;
            justify-content: space-between;
            flex-shrink: 0;
            position: relative;
        }
        
        .agent-details-header::before {
            content: '';
            position: absolute;
            bottom: 0;
            left: 0;
            right: 0;
            height: 1px;
            background: linear-gradient(90deg, transparent, rgba(0, 212, 255, 0.4), transparent);
        }
        
        .agent-details-title {
            color: #00d4ff;
            font-size: 14px;
            font-weight: 500;
            text-shadow: 0 0 8px rgba(0, 212, 255, 0.3);
            display: flex;
            align-items: center;
            gap: 8px;
            letter-spacing: 0.5px;
            text-transform: uppercase;
        }
        
        .agent-details-close {
            background: transparent;
            border: 1px solid rgba(0, 212, 255, 0.3);
            color: rgba(0, 212, 255, 0.6);
            width: 20px;
            height: 20px;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 12px;
            transition: all 0.3s ease;
            clip-path: polygon(20% 0%, 80% 0%, 100% 20%, 100% 80%, 80% 100%, 20% 100%, 0% 80%, 0% 20%);
        }
        
        .agent-details-close:hover {
            background: rgba(0, 212, 255, 0.1);
            border-color: rgba(0, 212, 255, 0.8);
            color: #00d4ff;
            box-shadow: inset 0 0 8px rgba(0, 212, 255, 0.2);
        }
        
        .agent-details-content {
            padding: 15px 20px;
            flex: 1;
            color: #e0e6ed;
            font-size: 12px;
        }
        
        .agent-stat-section {
            margin-bottom: 15px;
            padding: 12px;
            background: rgba(255, 255, 255, 0.03);
            border-radius: 8px;
            border: 1px solid rgba(0, 212, 255, 0.1);
        }
        
        .agent-stat-title {
            color: #00d4ff;
            font-size: 11px;
            font-weight: 600;
            margin-bottom: 8px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .agent-stat-grid {
            display: flex;
            flex-direction: column;
            gap: 6px;
        }
        
        .agent-stat-item {
            display: flex;
            align-items: center;
            gap: 6px;
            padding: 6px 8px;
            background: rgba(0, 212, 255, 0.05);
            border-radius: 4px;
            border: 1px solid rgba(0, 212, 255, 0.1);
        }
        
        .agent-stat-icon {
            font-size: 12px;
            width: 16px;
            text-align: center;
            flex-shrink: 0;
        }
        
        .agent-stat-text {
            font-size: 10px;
            line-height: 1.3;
            flex: 1;
        }
        
        .agent-stat-text strong {
            color: #00d4ff;
            display: block;
            margin-bottom: 1px;
        }
        
        @keyframes slideDown {
            from {
                opacity: 0;
                transform: translateY(-10px);
            }
            to {
                opacity: 1;
                transform: translateY(0);
            }
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
        
        /* Academy Sidebar Styles */
        .academy-selector {
            background: rgba(255,255,255,0.05);
            border-radius: 12px;
            padding: 15px;
            margin: 20px;
            border: 1px solid rgba(255,255,255,0.1);
        }
        
        .academy-selector h3 {
            margin: 0 0 15px 0;
            color: #E0E0E0;
            font-size: 14px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .academy-training-list {
            max-height: 200px;
            overflow-y: auto;
        }
        
        .academy-training-item {
            background: linear-gradient(135deg, #1a1a2e 0%, #2a2a3e 100%);
            border: 1px solid #7c4dff;
            border-radius: 8px;
            padding: 12px;
            margin-bottom: 8px;
            box-shadow: 0 2px 10px rgba(124, 77, 255, 0.15);
            transition: all 0.3s ease;
        }
        
        .academy-training-item:hover {
            border-color: #00d4ff;
            box-shadow: 0 4px 15px rgba(0, 212, 255, 0.25);
        }
        
        .academy-training-item .training-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 8px;
        }
        
        .academy-training-item .training-name {
            color: #ffffff;
            font-weight: bold;
            font-size: 13px;
        }
        
        .academy-training-item .training-status {
            color: #FF9800;
            font-size: 10px;
            background: rgba(255, 152, 0, 0.15);
            padding: 2px 6px;
            border-radius: 10px;
            border: 1px solid rgba(255, 152, 0, 0.3);
        }
        
        .academy-training-item .training-progress {
            font-size: 11px;
            color: #ccc;
            margin-bottom: 6px;
        }
        
        .academy-training-item .training-bar {
            width: 100%;
            height: 4px;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 2px;
            overflow: hidden;
        }
        
        .academy-training-item .training-fill {
            height: 100%;
            background: linear-gradient(90deg, #FF9800, #FF5722);
            border-radius: 2px;
            transition: width 0.5s ease;
        }
        
        .academy-stats-display {
            background: rgba(255,255,255,0.05);
            border-radius: 12px;
            padding: 15px;
            margin: 20px;
            border: 1px solid rgba(255,255,255,0.1);
        }
        
        .academy-stats-display h3 {
            margin: 0 0 15px 0;
            color: #E0E0E0;
            font-size: 14px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .academy-graduates {
            background: rgba(255,255,255,0.05);
            border-radius: 12px;
            padding: 15px;
            margin: 20px;
            border: 1px solid rgba(255,255,255,0.1);
        }
        
        .academy-graduates h3 {
            margin: 0 0 15px 0;
            color: #E0E0E0;
            font-size: 14px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .academy-graduates-list {
            max-height: 150px;
            overflow-y: auto;
        }
        
        .academy-graduate-item {
            background: rgba(255,255,255,0.08);
            border-radius: 6px;
            padding: 8px;
            margin-bottom: 6px;
            border-left: 3px solid #4CAF50;
            transition: all 0.2s ease;
        }
        
        .academy-graduate-item:hover {
            background: rgba(255,255,255,0.12);
        }
        
        .academy-graduate-item .graduate-name {
            color: #4CAF50;
            font-weight: bold;
            font-size: 12px;
        }
        
        .academy-graduate-item .graduate-info {
            color: #888;
            font-size: 10px;
            margin-top: 3px;
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
                    <div style="
                        position: relative; 
                        width: 28px; 
                        height: 28px; 
                        margin-right: 10px;
                        flex-shrink: 0;
                    ">
                        <div style="
                            position: absolute;
                            left: 0;
                            top: 0;
                            width: 28px; 
                            height: 28px; 
                            border: 2px solid #00d4ff; 
                            border-radius: 50%; 
                            background: transparent;
                            box-shadow: 0 0 10px rgba(0, 212, 255, 0.3);
                            z-index: 1;
                        "></div>
                        <span class="connection-status connecting" id="connection-status" style="
                            position: absolute;
                            left: 14px;
                            top: 14px;
                            transform: translate(-50%, -50%);
                            z-index: 0;
                        "></span>
                    </div>
                    <div class="logo-text" style="color: #00d4ff; text-shadow: 0 0 8px rgba(0, 212, 255, 0.4);">
                        continuum
                    </div>
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
                    💬 Active
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
                        <div class="cost-value" id="cost-total">$${this.continuum.costTracker ? this.continuum.costTracker.getTotal().toFixed(2) : '0.00'}</div>
                        <div class="cost-label">Cost</div>
                    </div>
                </div>
                <button onclick="clearChatHistory()" style="
                    width: 100%;
                    margin-top: 10px;
                    padding: 6px 10px;
                    background: rgba(244, 67, 54, 0.2);
                    border: 1px solid #F44336;
                    color: #F44336;
                    border-radius: 6px;
                    font-size: 10px;
                    cursor: pointer;
                    transition: all 0.2s ease;
                " onmouseover="this.style.background='rgba(244, 67, 54, 0.3)'" 
                   onmouseout="this.style.background='rgba(244, 67, 54, 0.2)'">
                    🗑️ Clear History
                </button>
            </div>
            
            <!-- Room-specific Content -->
            <div id="room-content">
                <!-- General Room Content -->
                <div id="general-room" class="room-content">
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
                            
                            <div class="agent-item" id="agent-PlannerAI">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #9C27B0, #673AB7);" onclick="selectAgent('PlannerAI')">
                                    📋
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info" onclick="selectAgent('PlannerAI')">
                                    <div class="agent-name">
                                        PlannerAI
                                        <button class="agent-dropdown-btn" onclick="event.stopPropagation(); toggleAgentInfo('PlannerAI')" title="Agent details"></button>
                                    </div>
                                    <div class="agent-role">Strategy & web commands</div>
                                </div>
                                <div class="agent-actions">
                                    <button class="agent-action-btn retrain" onclick="event.stopPropagation(); retrainAgent('PlannerAI')" title="Retrain PlannerAI">
                                        🎓
                                    </button>
                                </div>
                            </div>
                            
                            <div class="agent-item" id="agent-CodeAI">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #FF5722, #F44336);" onclick="selectAgent('CodeAI')">
                                    💻
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info" onclick="selectAgent('CodeAI')">
                                    <div class="agent-name">
                                        CodeAI
                                        <button class="agent-dropdown-btn" onclick="event.stopPropagation(); toggleAgentInfo('CodeAI')" title="Agent details"></button>
                                    </div>
                                    <div class="agent-role">Code analysis & debugging</div>
                                </div>
                                <div class="agent-actions">
                                    <button class="agent-action-btn retrain" onclick="event.stopPropagation(); retrainAgent('CodeAI')" title="Retrain CodeAI">
                                        🎓
                                    </button>
                                </div>
                            </div>
                            
                            <div class="agent-item" id="agent-GeneralAI">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #4CAF50, #8BC34A);" onclick="selectAgent('GeneralAI')">
                                    💬
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info" onclick="selectAgent('GeneralAI')">
                                    <div class="agent-name">
                                        GeneralAI
                                        <button class="agent-dropdown-btn" onclick="event.stopPropagation(); toggleAgentInfo('GeneralAI')" title="Agent details"></button>
                                    </div>
                                    <div class="agent-role">General assistance</div>
                                </div>
                                <div class="agent-actions">
                                    <button class="agent-action-btn retrain" onclick="event.stopPropagation(); retrainAgent('GeneralAI')" title="Retrain GeneralAI">
                                        🎓
                                    </button>
                                </div>
                            </div>
                            
                            <div class="agent-item" id="agent-ProtocolSheriff">
                                <div class="agent-avatar" style="background: linear-gradient(135deg, #FF9800, #FFC107);" onclick="selectAgent('ProtocolSheriff')">
                                    🛡️
                                    <div class="agent-status online"></div>
                                </div>
                                <div class="agent-info" onclick="selectAgent('ProtocolSheriff')">
                                    <div class="agent-name">
                                        Protocol Sheriff
                                        <button class="agent-dropdown-btn" onclick="event.stopPropagation(); toggleAgentInfo('ProtocolSheriff')" title="Agent details"></button>
                                    </div>
                                    <div class="agent-role">Response validation</div>
                                </div>
                                <div class="agent-actions">
                                    <button class="agent-action-btn retrain" onclick="event.stopPropagation(); retrainAgent('ProtocolSheriff')" title="Retrain ProtocolSheriff">
                                        🎓
                                    </button>
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
                    <!-- Academy Training Status -->
                    <div class="academy-selector">
                        <h3>Training Status</h3>
                        <div id="academy-training-list" class="academy-training-list">
                            <div class="loading">Loading training sessions...</div>
                        </div>
                    </div>
                    
                    <!-- Academy Actions -->
                    <div class="academy-actions" style="margin-top: 20px;">
                        <button class="academy-button" onclick="sendSheriffToAcademy()">
                            🛡️ Deploy Sheriff
                        </button>
                        <button class="academy-button" onclick="trainCustomPersona()">
                            🎓 Custom Training
                        </button>
                        <button class="academy-button" onclick="startPendingTraining()" id="pending-training-btn" style="display: none;">
                            🚀 Start Pending Training
                        </button>
                    </div>
                    
                    <!-- Academy Stats -->
                    <div class="academy-stats-display" style="margin-top: 20px;">
                        <h3>Academy Statistics</h3>
                        <div id="academy-stats" class="cost-stats">
                            <div class="cost-item">
                                <div class="cost-value" id="academy-active-count">0</div>
                                <div class="cost-label">Training</div>
                            </div>
                            <div class="cost-item">
                                <div class="cost-value" id="academy-graduated-count">0</div>
                                <div class="cost-label">Graduated</div>
                            </div>
                        </div>
                    </div>
                    
                    <!-- Recent Graduates -->
                    <div class="academy-graduates" style="margin-top: 20px;">
                        <h3>Recent Graduates</h3>
                        <div id="academy-graduates-list" class="academy-graduates-list">
                            <div class="loading">No recent graduates</div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
        
        <!-- Main Chat -->
        <div class="main-content">
            <div class="chat-header">
                <div class="chat-header-content">
                    <div class="chat-title" id="chat-title">Active Chat</div>
                    <div class="chat-subtitle" id="chat-subtitle">Talk to specialized AI agents</div>
                </div>
                <div style="display: flex; align-items: flex-start; gap: 10px;">
                    <button class="clear-chat-btn" onclick="clearChatHistory()">
                        🗑️ Clear
                    </button>
                    <div class="version-badge" style="
                        background: rgba(0, 212, 255, 0.1);
                        border: 1px solid #00d4ff;
                        color: #00d4ff;
                        padding: 4px 8px;
                        border-radius: 12px;
                        font-size: 11px;
                        font-weight: bold;
                    ">
                        v${require('../../package.json').version || '1.0.0'}
                    </div>
                    <button class="web-browser-btn" id="web-browser-btn" onclick="toggleWebBrowser()" style="
                        background: rgba(102, 102, 102, 0.1);
                        border: 1px solid #666;
                        color: #666;
                        padding: 4px 8px;
                        border-radius: 8px;
                        font-size: 11px;
                        cursor: pointer;
                        transition: all 0.3s ease;
                        display: flex;
                        align-items: center;
                        gap: 4px;
                    " title="Toggle Web Browser Tab">
                        🌐 <span id="web-status">Web</span>
                    </button>
                </div>
            </div>
            
            <div class="chat-container" id="chat"></div>
            
            <!-- Web Browser Container -->
            <div class="web-browser-container" id="web-browser-container" style="
                display: none;
                flex: 1;
                background: #fff;
                border-radius: 8px;
                overflow: hidden;
                margin: 0;
                position: relative;
            ">
                <div class="web-browser-header" style="
                    background: #f5f5f5;
                    padding: 8px 12px;
                    border-bottom: 1px solid #ddd;
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    font-size: 12px;
                    color: #666;
                ">
                    <div class="web-browser-controls" style="display: flex; gap: 4px;">
                        <div style="width: 12px; height: 12px; background: #ff5f56; border-radius: 50%;"></div>
                        <div style="width: 12px; height: 12px; background: #ffbd2e; border-radius: 50%;"></div>
                        <div style="width: 12px; height: 12px; background: #27ca3f; border-radius: 50%;"></div>
                    </div>
                    <div class="web-address-bar" style="
                        flex: 1;
                        background: #fff;
                        border: 1px solid #ddd;
                        border-radius: 4px;
                        padding: 4px 8px;
                        font-size: 11px;
                        color: #333;
                    " id="web-address-bar">
                        about:blank
                    </div>
                    <button onclick="reloadWebBrowser()" style="
                        background: #007AFF;
                        border: none;
                        color: white;
                        padding: 4px 8px;
                        border-radius: 4px;
                        cursor: pointer;
                        font-size: 10px;
                    ">⟳</button>
                    <button onclick="toggleWebBrowser()" style="
                        background: #FF3B30;
                        border: none;
                        color: white;
                        padding: 4px 8px;
                        border-radius: 4px;
                        cursor: pointer;
                        font-size: 10px;
                    ">✕</button>
                </div>
                <iframe 
                    id="web-browser-iframe"
                    src="about:blank"
                    style="
                        width: 100%;
                        height: calc(100% - 40px);
                        border: none;
                        background: white;
                    "
                    sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-top-navigation"
                ></iframe>
            </div>
            
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

    <!-- Cyberpunk Agent Details Glass Panel -->
    <div id="agent-details-panel" class="agent-details-panel">
        <div class="agent-details-header">
            <div class="agent-details-title">
                <span id="agent-details-avatar">🤖</span>
                <span id="agent-details-name">Agent Details</span>
            </div>
            <button class="agent-details-close" onclick="closeAgentDetails()">×</button>
        </div>
        <div class="agent-details-content">
            <div class="agent-stat-section">
                <div class="agent-stat-title">Performance Metrics</div>
                <div class="agent-stat-grid">
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">🎯</div>
                        <div class="agent-stat-text"><strong>Specialization:</strong> <span id="detail-specialization">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">📊</div>
                        <div class="agent-stat-text"><strong>Accuracy:</strong> <span id="detail-accuracy">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">⚡</div>
                        <div class="agent-stat-text"><strong>Response Time:</strong> <span id="detail-response-time">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">🛠️</div>
                        <div class="agent-stat-text"><strong>Tools:</strong> <span id="detail-tools">Loading...</span></div>
                    </div>
                </div>
            </div>
            
            <div class="agent-stat-section">
                <div class="agent-stat-title">Training Status</div>
                <div class="agent-stat-grid">
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">🎓</div>
                        <div class="agent-stat-text"><strong>Last Training:</strong> <span id="detail-last-training">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">💡</div>
                        <div class="agent-stat-text"><strong>Strength:</strong> <span id="detail-strength">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">🧠</div>
                        <div class="agent-stat-text"><strong>Model:</strong> <span id="detail-model">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">🔄</div>
                        <div class="agent-stat-text"><strong>Status:</strong> <span id="detail-status">Loading...</span></div>
                    </div>
                </div>
            </div>
            
            <div class="agent-stat-section">
                <div class="agent-stat-title">Capabilities</div>
                <div class="agent-stat-grid">
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">💻</div>
                        <div class="agent-stat-text"><strong>Languages:</strong> <span id="detail-languages">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">🌐</div>
                        <div class="agent-stat-text"><strong>Web Access:</strong> <span id="detail-web-access">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">📁</div>
                        <div class="agent-stat-text"><strong>File Operations:</strong> <span id="detail-file-ops">Loading...</span></div>
                    </div>
                    <div class="agent-stat-item">
                        <div class="agent-stat-icon">🔧</div>
                        <div class="agent-stat-text"><strong>Advanced Features:</strong> <span id="detail-advanced">Loading...</span></div>
                    </div>
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
        
        // Chat history persistence
        const STORAGE_KEY = 'continuum_chat_history';
        const MAX_STORED_MESSAGES = 100;
        
        function saveChatHistory() {
            try {
                const historyData = {
                    roomMessages,
                    messageHistory: messageHistory.slice(-MAX_STORED_MESSAGES),
                    timestamp: Date.now(),
                    currentRoom,
                    selectedAgent,
                    pendingRetrain: window.pendingRetrain || null
                };
                localStorage.setItem(STORAGE_KEY, JSON.stringify(historyData));
            } catch (error) {
                console.error('Failed to save chat history:', error);
            }
        }
        
        function loadChatHistory() {
            try {
                const stored = localStorage.getItem(STORAGE_KEY);
                if (!stored) return false;
                
                const historyData = JSON.parse(stored);
                
                // Don't load history older than 24 hours
                if (Date.now() - historyData.timestamp > 24 * 60 * 60 * 1000) {
                    localStorage.removeItem(STORAGE_KEY);
                    return false;
                }
                
                // Restore chat data
                if (historyData.roomMessages) {
                    roomMessages = historyData.roomMessages;
                }
                if (historyData.messageHistory) {
                    messageHistory = historyData.messageHistory;
                }
                if (historyData.currentRoom) {
                    currentRoom = historyData.currentRoom;
                }
                if (historyData.selectedAgent) {
                    selectedAgent = historyData.selectedAgent;
                }
                if (historyData.pendingRetrain) {
                    window.pendingRetrain = historyData.pendingRetrain;
                    showPendingTrainingButton();
                }
                
                console.log(\`📚 Restored \${Object.values(roomMessages).flat().length} messages from local storage\`);
                return true;
            } catch (error) {
                console.error('Failed to load chat history:', error);
                localStorage.removeItem(STORAGE_KEY);
                return false;
            }
        }
        
        function clearChatHistory() {
            localStorage.removeItem(STORAGE_KEY);
            roomMessages = { general: [], academy: [] };
            messageHistory = [];
            window.pendingRetrain = null;
            hidePendingTrainingButton();
            
            // Clear current chat display
            const chat = document.getElementById('chat');
            if (chat) chat.innerHTML = '';
            
            addMessage('System', '🗑️ Chat history cleared', 'ai', false, true);
        }
        
        // Update connection status indicator
        function updateConnectionStatus(status) {
            const indicator = document.getElementById('connection-status');
            if (indicator) {
                indicator.className = 'connection-status ' + status;
            }
            
            // Update subtle status text in chat subtitle
            const subtitle = document.getElementById('chat-subtitle');
            if (subtitle) {
                const baseText = subtitle.textContent.split(' •')[0]; // Remove existing status
                const statusText = status === 'connected' ? '• Connected' : 
                                 status === 'connecting' ? '• Connecting...' : 
                                 '• Reconnecting...';
                subtitle.textContent = baseText + ' ' + statusText;
            }
        }

        // Remote Agent Management
        let allAvailableAgents = [];
        
        function updateAvailableAgents(agents) {
            console.log('🤖 Updating available agents:', agents);
            allAvailableAgents = agents;
            renderAgentList();
            
            // BRIDGE: Also update any AgentSelector web components
            const agentSelectors = document.querySelectorAll('agent-selector');
            agentSelectors.forEach(selector => {
                if (selector && typeof selector.updateRemoteAgents === 'function') {
                    console.log('🔄 Updating AgentSelector component with remote agents');
                    selector.updateRemoteAgents(agents.filter(agent => agent.source === 'remote'));
                }
            });
        }
        
        function renderAgentList() {
            const agentList = document.querySelector('.agent-list');
            if (!agentList) return;
            
            // Keep the existing Auto Route option
            const existingAgents = agentList.innerHTML;
            
            // Separate remote agents by type
            const remoteAIs = allAvailableAgents.filter(agent => 
                agent.source === 'remote' && (agent.type === 'ai' || agent.type === 'system'));
            const remoteHumans = allAvailableAgents.filter(agent => 
                agent.source === 'remote' && (agent.type === 'human' || agent.type === 'user'));
            
            let additionalHTML = '';
            
            // Add remote humans section
            if (remoteHumans.length > 0) {
                const humanHTML = remoteHumans.map(agent => generateAgentHTML(agent)).join('');
                additionalHTML += 
                    '<div style="margin: 15px 0; padding: 10px 0; border-top: 1px solid rgba(255,255,255,0.1);">' +
                    '<div style="color: #ff6b6b; font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 10px;">⚡ Network Operators</div>' +
                    humanHTML +
                    '</div>';
            }
            
            // Add remote AIs section
            if (remoteAIs.length > 0) {
                const aiHTML = remoteAIs.map(agent => generateAgentHTML(agent)).join('');
                additionalHTML += 
                    '<div style="margin: 15px 0; padding: 10px 0; border-top: 1px solid rgba(255,255,255,0.1);">' +
                    '<div style="color: #00d4ff; font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 10px;">🌐 Network AI</div>' +
                    aiHTML +
                    '</div>';
            }
            
            // Add remote agents after local ones
            if (additionalHTML) {
                const localAgentsEnd = agentList.innerHTML.lastIndexOf('</div>') + 6;
                const beforeRemote = agentList.innerHTML.substring(0, localAgentsEnd);
                const afterRemote = agentList.innerHTML.substring(localAgentsEnd);
                
                agentList.innerHTML = beforeRemote + additionalHTML + afterRemote;
            }
        }
        
        function generateAgentHTML(agent) {
            const statusColor = agent.status === 'connected' ? 'online' : 
                              agent.status === 'busy' ? 'busy' : 'offline';
            const avatarEmoji = getAgentEmoji(agent);
            const agentId = 'agent-remote-' + agent.id;
            const isAI = agent.type === 'ai' || agent.type === 'system';
            const isHuman = agent.type === 'human' || agent.type === 'user';
            
            // Different styling for AI vs Human
            const avatarGradient = isHuman 
                ? 'linear-gradient(135deg, #ff6b6b, #ee5a5a)' // Red for humans
                : 'linear-gradient(135deg, #00d4ff, #0099cc)'; // Blue for AI
            
            return \`
                <div class="agent-item" id="\${agentId}" onclick="selectAgent('\${agent.id}')">
                    <div class="agent-avatar" style="background: \${avatarGradient};">
                        \${avatarEmoji}
                        <div class="agent-status \${statusColor}"></div>
                    </div>
                    <div class="agent-info">
                        <div class="agent-name">
                            \${agent.name}
                            <span style="font-size: 9px; color: #666; margin-left: 6px; padding: 1px 4px; background: rgba(0,0,0,0.3); border-radius: 2px;" title="\${isHuman ? 'Human Operator' : 'AI Agent'}">\${isHuman ? 'H' : 'AI'}</span>
                        </div>
                        <div class="agent-role">\${agent.capabilities?.join(', ') || agent.role || 'General'}</div>
                        <div style="font-size: 10px; color: #666; margin-top: 2px;">
                            \${agent.hostInfo?.hostname || agent.host || 'Local'} • 
                            \${agent.messageCount || 0} msgs
                        </div>
                    </div>
                </div>
            \`;
        }
        
        function getAgentEmoji(agent) {
            // Handle human vs AI differently
            if (agent.type === 'human' || agent.type === 'user') {
                return '👤'; // Clean human icon
            }
            
            const capabilities = agent.capabilities || [];
            if (capabilities.includes('code')) return '💻';
            if (capabilities.includes('analysis')) return '📊';
            if (capabilities.includes('planning')) return '📋';
            if (capabilities.includes('creative')) return '🎨';
            if (capabilities.includes('reasoning')) return '🧠';
            if (capabilities.includes('visual')) return '👁️';
            if (capabilities.includes('web')) return '🌐';
            if (agent.type === 'system') return '⚡';
            return '🤖';
        }
        
        // Handle agent selection including remote agents
        function selectAgent(agentId) {
            console.log('🎯 Selected agent:', agentId);
            
            // Remove selected class from all agents
            document.querySelectorAll('.agent-item').forEach(item => {
                item.classList.remove('selected');
            });
            
            // Add selected class to clicked agent
            const selectedElement = document.getElementById('agent-' + agentId) || 
                                   document.getElementById('agent-remote-' + agentId);
            if (selectedElement) {
                selectedElement.classList.add('selected');
            }
            
            // Update global selected agent
            selectedAgent = agentId;
            
            // Update chat title to show selected agent
            const agent = allAvailableAgents.find(a => a.id === agentId);
            if (agent) {
                setChatTitle(\`Chat with \${agent.name}\${agent.source === 'remote' ? ' 🌐' : ''}\`);
            } else {
                setChatTitle('Chat with ' + agentId);
            }
        }

        // AI Cursor Control - Make HAL 9000 indicator become the AI's mouse
        let aiCursorActive = false;
        let aiCursorOriginalParent = null;
        let aiCursorOriginalStyle = null;

        function activateAICursor() {
            const indicator = document.getElementById('connection-status');
            if (!indicator || aiCursorActive) return;

            console.log('🤖 AI Cursor activated - HAL 9000 is now the mouse');
            
            // Store original state and precise position
            aiCursorOriginalParent = indicator.parentElement;
            aiCursorOriginalStyle = indicator.style.cssText;
            
            // Activate cursor mode
            aiCursorActive = true;
            indicator.classList.add('ai-cursor');
            
            // Apply Continuon-style centering (left: 33%; top: 33%)
            const homeX = window.innerWidth * 0.33; // 33% from left
            const homeY = window.innerHeight * 0.33; // 33% from top
            
            // Position at Continuon-style center location
            indicator.style.left = homeX + 'px';
            indicator.style.top = homeY + 'px';
            
            console.log('Continuon activated at centered position (' + homeX + ', ' + homeY + ') - 33% style');
            
            return indicator;
        }

        function deactivateAICursor() {
            const indicator = document.getElementById('connection-status');
            if (!indicator || !aiCursorActive) return;

            console.log('🤖 AI Cursor deactivated - Continuon returning to base');
            
            // Smooth return animation to Continuon-style centered position
            const homeContainer = aiCursorOriginalParent;
            if (homeContainer) {
                // Return to the same Continuon-style center position (33%, 33%)
                const homeX = window.innerWidth * 0.33;
                const homeY = window.innerHeight * 0.33;
                
                // Animate return to Continuon center
                indicator.style.transition = 'left 0.8s cubic-bezier(0.25, 0.46, 0.45, 0.94), top 0.8s cubic-bezier(0.25, 0.46, 0.45, 0.94)';
                indicator.style.left = homeX + 'px';
                indicator.style.top = homeY + 'px';
                
                // After animation, restore original state
                setTimeout(() => {
                    indicator.classList.remove('ai-cursor', 'ai-cursor-click');
                    
                    if (aiCursorOriginalStyle) {
                        indicator.style.cssText = aiCursorOriginalStyle;
                    }
                    
                    console.log('🟢 Continuon returned to centered base (33% style)');
                }, 800);
            } else {
                // Immediate restoration if no home container
                indicator.classList.remove('ai-cursor', 'ai-cursor-click');
                if (aiCursorOriginalStyle) {
                    indicator.style.cssText = aiCursorOriginalStyle;
                }
            }
            
            // Reset state
            aiCursorActive = false;
            aiCursorOriginalParent = null;
            aiCursorOriginalStyle = null;
        }

        function moveAICursor(x, y, smooth = true) {
            const indicator = document.getElementById('connection-status');
            if (!indicator || !aiCursorActive) return;

            if (smooth) {
                // Smooth Bezier-like movement
                const currentX = parseInt(indicator.style.left) || 0;
                const currentY = parseInt(indicator.style.top) || 0;
                
                // Create natural curve movement
                const deltaX = x - currentX;
                const deltaY = y - currentY;
                const distance = Math.sqrt(deltaX * deltaX + deltaY * deltaY);
                
                if (distance > 50) {
                    // For longer distances, create a curved path
                    const steps = Math.min(20, Math.max(10, distance / 50));
                    const stepDelay = Math.max(30, 200 / steps);
                    
                    for (let i = 1; i <= steps; i++) {
                        setTimeout(() => {
                            const t = i / steps;
                            // Bezier curve calculation
                            const easeT = t * t * (3 - 2 * t); // Smoothstep
                            
                            // Add slight curve for natural movement
                            const midX = (currentX + x) / 2 + (Math.random() - 0.5) * 50;
                            const midY = (currentY + y) / 2 + (Math.random() - 0.5) * 50;
                            
                            const bezierX = currentX + t * deltaX + (1 - t) * t * (midX - currentX - t * deltaX);
                            const bezierY = currentY + t * deltaY + (1 - t) * t * (midY - currentY - t * deltaY);
                            
                            indicator.style.left = Math.round(bezierX) + 'px';
                            indicator.style.top = Math.round(bezierY) + 'px';
                            
                            if (i === steps) {
                                // Ensure final position is exact
                                indicator.style.left = x + 'px';
                                indicator.style.top = y + 'px';
                            }
                        }, i * stepDelay);
                    }
                } else {
                    // For short distances, move directly
                    indicator.style.left = x + 'px';
                    indicator.style.top = y + 'px';
                }
            } else {
                // Instant movement
                indicator.style.left = x + 'px';
                indicator.style.top = y + 'px';
            }
        }

        function aiCursorClick(x, y) {
            const indicator = document.getElementById('connection-status');
            if (!indicator || !aiCursorActive) return;

            // Move to position if not already there
            moveAICursor(x, y, true);
            
            // Add click animation after movement
            setTimeout(() => {
                indicator.classList.add('ai-cursor-click');
                
                // Remove click animation after it completes
                setTimeout(() => {
                    indicator.classList.remove('ai-cursor-click');
                }, 300);
                
                // Simulate actual click at the position
                const elementAtPoint = document.elementFromPoint(x, y);
                if (elementAtPoint && elementAtPoint !== indicator) {
                    console.log('🖱️ AI clicking element:', elementAtPoint.tagName, elementAtPoint.className);
                    elementAtPoint.click();
                }
            }, 500); // Wait for movement to complete
        }

        function aiCursorScreenshot() {
            if (!aiCursorActive) {
                activateAICursor();
            }
            
            // Create screenshot feedback rectangle
            showScreenshotFeedback();
            
            // Move to a corner briefly to indicate screenshot
            const indicator = document.getElementById('connection-status');
            if (indicator) {
                const originalX = parseInt(indicator.style.left) || 0;
                const originalY = parseInt(indicator.style.top) || 0;
                
                // Flash to top-right corner
                moveAICursor(window.innerWidth - 50, 50, true);
                
                setTimeout(() => {
                    // Flash effect for screenshot
                    indicator.style.background = 'radial-gradient(circle, #ffffff, #00ff41)';
                    indicator.style.boxShadow = '0 0 100px rgba(255, 255, 255, 1)';
                    
                    setTimeout(() => {
                        // Return to normal appearance and position
                        indicator.style.background = '';
                        indicator.style.boxShadow = '';
                        moveAICursor(originalX, originalY, true);
                    }, 200);
                }, 1000);
            }
        }

        // Import Screenshot Feedback Module
        // This will be loaded via script tag in the HTML
        function initializeScreenshotFeedback() {
            // The ScreenshotFeedback module will be loaded separately
            console.log('📸 Screenshot feedback system initialized');
        }

        // Global function to trigger screenshot feedback (callable from commands)
        window.triggerScreenshotFeedback = function() {
            if (typeof window.ScreenshotFeedback !== 'undefined') {
                return new window.ScreenshotFeedback().show();
            } else {
                console.warn('ScreenshotFeedback module not loaded - falling back to basic implementation');
                // Fallback implementation
                showBasicScreenshotFeedback();
            }
        };

        // Web Browser Control Functions
        let webBrowserActive = false;

        function toggleWebBrowser() {
            const chatContainer = document.getElementById('chat');
            const webContainer = document.getElementById('web-browser-container');
            const webBtn = document.getElementById('web-browser-btn');
            const webStatus = document.getElementById('web-status');

            webBrowserActive = !webBrowserActive;

            if (webBrowserActive) {
                // Show web browser, hide chat
                chatContainer.style.display = 'none';
                webContainer.style.display = 'flex';
                webBtn.style.background = 'rgba(0, 212, 255, 0.2)';
                webBtn.style.borderColor = '#00d4ff';
                webBtn.style.color = '#00d4ff';
                webStatus.textContent = 'Active';
                
                console.log('🌐 Web browser activated in Continuum');
                
                // Initialize with a default page if blank
                const iframe = document.getElementById('web-browser-iframe');
                if (iframe.src === 'about:blank') {
                    navigateWebBrowser('https://example.com');
                }
            } else {
                // Show chat, hide web browser
                chatContainer.style.display = 'flex';
                webContainer.style.display = 'none';
                webBtn.style.background = 'rgba(102, 102, 102, 0.1)';
                webBtn.style.borderColor = '#666';
                webBtn.style.color = '#666';
                webStatus.textContent = 'Web';
                
                console.log('💬 Chat view restored in Continuum');
            }
        }

        function navigateWebBrowser(url) {
            const iframe = document.getElementById('web-browser-iframe');
            const addressBar = document.getElementById('web-address-bar');
            
            // Ensure URL has protocol
            if (!url.match(/^https?:\\/\\//)) {
                url = 'https://' + url;
            }
            
            console.log(\`🌐 Navigating to: \${url}\`);
            iframe.src = url;
            addressBar.textContent = url;
            
            // Activate Continuon for web interaction
            if (typeof activateAICursor === 'function') {
                setTimeout(() => {
                    activateAICursor();
                    console.log('🟢 Continuon activated for web interaction');
                }, 1000);
            }
        }

        function reloadWebBrowser() {
            const iframe = document.getElementById('web-browser-iframe');
            iframe.src = iframe.src; // Reload current page
            console.log('🔄 Web browser reloaded');
        }

        // Web Browser Continuon Integration
        function getWebBrowserPosition(x, y) {
            const webContainer = document.getElementById('web-browser-container');
            const iframe = document.getElementById('web-browser-iframe');
            
            if (!webBrowserActive || !webContainer || !iframe) {
                return { x: x, y: y };
            }
            
            // Convert screen coordinates to iframe coordinates
            const containerRect = webContainer.getBoundingClientRect();
            const iframeRect = iframe.getBoundingClientRect();
            
            const relativeX = x - iframeRect.left;
            const relativeY = y - iframeRect.top;
            
            return {
                x: Math.max(0, Math.min(relativeX, iframeRect.width)),
                y: Math.max(0, Math.min(relativeY, iframeRect.height)),
                isInIframe: relativeX >= 0 && relativeX <= iframeRect.width && 
                           relativeY >= 0 && relativeY <= iframeRect.height
            };
        }

        // Global web browser functions
        window.toggleWebBrowser = toggleWebBrowser;
        window.navigateWebBrowser = navigateWebBrowser;
        window.reloadWebBrowser = reloadWebBrowser;
        window.getWebBrowserPosition = getWebBrowserPosition;

        // Basic fallback implementation
        function showBasicScreenshotFeedback() {
            const feedback = document.createElement('div');
            feedback.style.cssText = \`
                position: fixed; top: 0; left: 0; right: 0; bottom: 0;
                border: 4px solid #00ff41; border-radius: 12px;
                background: rgba(0, 255, 65, 0.1); pointer-events: none;
                z-index: 9999; animation: basicFlash 2s ease-out forwards;
            \`;
            
            if (!document.getElementById('basic-flash-style')) {
                const style = document.createElement('style');
                style.id = 'basic-flash-style';
                style.textContent = \`
                    @keyframes basicFlash {
                        0% { opacity: 0; }
                        15% { opacity: 1; }
                        100% { opacity: 0; }
                    }
                \`;
                document.head.appendChild(style);
            }
            
            document.body.appendChild(feedback);
            setTimeout(() => feedback.remove(), 2000);
        }
        
        // Helper function to update chat title
        function setChatTitle(text) {
            const title = document.getElementById('chat-title');
            if (title) {
                title.textContent = text;
            }
        }
        
        // Version change detection and auto-reload via WebSocket
        let currentVersion = '${packageInfo.version}';
        
        function checkVersionUpdate(serverVersion) {
            if (serverVersion && serverVersion !== currentVersion) {
                console.log(\`🔄 Version update detected: \${currentVersion} → \${serverVersion}\`);
                console.log('🌐 Reloading browser to update interface...');
                
                // Show update notification
                addSystemMessage(\`🔄 Continuum updated to v\${serverVersion} - Reloading interface...\`);
                
                // Reload after brief delay with loop protection
                setTimeout(() => {
                    if (window.reloadTracker && window.reloadTracker.checkReloadLoop()) {
                        window.location.reload();
                    } else {
                        console.error('🚨 Reload blocked by safety mechanism');
                        addSystemMessage('🚨 Update reload blocked - too many rapid reloads detected');
                    }
                }, 2000);
                
                return true;
            }
            return false;
        }

        // Initialize WebSocket connection
        function initWebSocket() {
            // Prevent multiple connection attempts
            if (window.wsConnecting) {
                return;
            }
            window.wsConnecting = true;
            
            updateConnectionStatus('connecting');
            
            try {
                // Override console temporarily to suppress native WebSocket errors
                const originalConsoleError = console.error;
                console.error = function(...args) {
                    const msg = args.join(' ');
                    // Suppress native WebSocket connection errors
                    if (msg.includes('WebSocket connection') && msg.includes('failed')) {
                        return; // Silently ignore
                    }
                    originalConsoleError.apply(console, args);
                };
                
                ws = new WebSocket('ws://localhost:' + (window.location.port || '9000'));
                
                // Restore console after a brief delay
                setTimeout(() => {
                    console.error = originalConsoleError;
                }, 1000);
            } catch (error) {
                window.wsConnecting = false;
                console.error('🚨 WebSocket creation failed:', error.message);
                scheduleReconnect();
                return;
            }
            
            ws.onopen = function() {
                window.wsConnecting = false;
                console.log('🔌 Connected to Continuum v' + currentVersion);
                updateConnectionStatus('connected');
                
                // Reset all connection tracking on successful connection
                window.reconnectAttempts = 0;
                window.disconnectLogged = false;
                window.errorLogged = false;
                window.connectionErrorShown = false;
                
                // Register this tab/window with server
                const tabId = sessionStorage.getItem('continuum-tab-id') || Date.now().toString();
                sessionStorage.setItem('continuum-tab-id', tabId);
                
                // Send tab registration to server
                ws.send(JSON.stringify({
                    type: 'tabRegister',
                    tabId: tabId,
                    version: currentVersion,
                    url: window.location.href,
                    timestamp: new Date().toISOString()
                }));
                
                console.log(\`📱 Tab registered: \${tabId}\`);
            };
            
            ws.onmessage = function(event) {
                const data = JSON.parse(event.data);
                
                // Debug academy_status messages specifically
                if (data.type === 'academy_status') {
                    console.log('🎓 RAW WebSocket message received:', event.data);
                    console.log('🎓 PARSED data:', data);
                    console.log('🎓 Active training in parsed data:', data.status?.activeTraining?.length || 0);
                }
                
                handleWebSocketMessage(data);
            };
            
            ws.onclose = function(event) {
                window.wsConnecting = false;
                
                // Only log if not a clean close and haven't logged recently
                if (event.code !== 1000 && !window.disconnectLogged) {
                    console.log('🔌 Disconnected from Continuum (will retry silently in background)');
                    window.disconnectLogged = true;
                    
                    // Set a timer to allow logging again after 60 seconds
                    setTimeout(() => {
                        window.disconnectLogged = false;
                    }, 60000);
                }
                
                updateConnectionStatus('disconnected');
                scheduleReconnect();
            };
            
            ws.onerror = function(error) {
                window.wsConnecting = false;
                
                // Severely limit error logging - only log once per 5 minutes
                const now = Date.now();
                if (!window.lastErrorLog || (now - window.lastErrorLog) > 300000) {
                    console.error('🚨 WebSocket error (retrying in background)');
                    window.lastErrorLog = now;
                }
                updateConnectionStatus('disconnected');
            };
        }
        
        function scheduleReconnect() {
            // Prevent multiple reconnection timers
            if (window.reconnectTimer) {
                clearTimeout(window.reconnectTimer);
            }
            
            if (!window.reconnectAttempts) window.reconnectAttempts = 0;
            window.reconnectAttempts++;
            
            // Much longer delays - start at 30 seconds, max 10 minutes
            const baseDelay = 30000; // 30 seconds base delay  
            const maxDelay = 600000; // 10 minutes max delay
            const delay = Math.min(baseDelay * Math.pow(2, Math.min(window.reconnectAttempts - 1, 6)), maxDelay);
            
            // Only log reconnection schedule occasionally
            if (window.reconnectAttempts <= 3) {
                console.log('🔄 Will retry connection in ' + Math.round(delay/1000) + 's (attempt ' + window.reconnectAttempts + ')');
            }
            
            window.reconnectTimer = setTimeout(() => {
                window.reconnectTimer = null;
                initWebSocket();
            }, delay);
        }
        
        function handleWebSocketMessage(data) {
            console.log('🔥 CLIENT v0.2.1943: Raw WebSocket message received:', data.type, data);
            
            // SPECIAL DEBUG: Log execute_js messages prominently
            if (data.type === 'execute_js') {
                console.log('🚨 EXECUTE_JS MESSAGE DETECTED!', data);
            }
            
            // Handle version updates from server
            if (data.type === 'versionUpdate') {
                console.log('🔥 CLIENT: Version update received:', data.version);
                if (checkVersionUpdate(data.version)) {
                    return; // Browser will reload, stop processing
                }
            }
            
            // Handle JavaScript execution from server (both legacy and promise modes)
            if (data.type === 'execute_js' || data.type === 'execute_js_promise') {
                console.log('🔥 CLIENT v0.2.1943: EXECUTE_JS received!', data);
                console.log('🔥 CLIENT: JavaScript command:', data.data?.command);
                
                try {
                    if (data.data && data.data.command) {
                        console.log('🔥 CLIENT: About to execute JavaScript...');
                        console.log('🔥 CLIENT: JS Code:', data.data.command);
                        
                        // Capture console output during execution
                        const originalLog = console.log;
                        const originalError = console.error;
                        const originalWarn = console.warn;
                        const capturedOutput = [];
                        
                        // Override console methods to capture output
                        console.log = (...args) => {
                            capturedOutput.push({level: 'log', message: args.join(' ')});
                            originalLog.apply(console, args);
                        };
                        console.error = (...args) => {
                            capturedOutput.push({level: 'error', message: args.join(' ')});
                            originalError.apply(console, args);
                        };
                        console.warn = (...args) => {
                            capturedOutput.push({level: 'warn', message: args.join(' ')});
                            originalWarn.apply(console, args);
                        };
                        
                        // SAFE ASYNC EXECUTION: Handle promises and returns properly
                        console.log('🛰️ CLIENT: Executing probe telemetry command...');
                        
                        const executeAsync = async () => {
                            try {
                                // Check if command already contains return or is an expression
                                const commandText = data.data.command.trim();
                                
                                if (commandText.startsWith('return ')) {
                                    // Command already has return, wrap it in async function
                                    const asyncWrapper = new Function('return (async () => { ' + commandText + ' })();');
                                    return await asyncWrapper();
                                } else if (commandText.includes('await') || commandText.includes('Promise')) {
                                    // Contains async operations, wrap in async function
                                    const asyncWrapper = new Function('return (async () => { return ' + commandText + ' })();');
                                    return await asyncWrapper();
                                } else {
                                    // Simple expression, evaluate directly
                                    return eval(commandText);
                                }
                            } catch (error) {
                                // If all else fails, try wrapping the entire command
                                try {
                                    const fallbackWrapper = new Function(data.data.command);
                                    return fallbackWrapper();
                                } catch (fallbackError) {
                                    throw error; // Throw original error
                                }
                            }
                        };
                        
                        executeAsync().then(result => {
                            // Restore original console methods
                            console.log = originalLog;
                            console.error = originalError;
                            console.warn = originalWarn;
                            
                            console.log('🔥 CLIENT: JavaScript executed successfully!');
                            console.log('🔥 CLIENT: Captured output:', capturedOutput);
                            
                            // Send execution result and console output back to server
                            if (ws && ws.readyState === WebSocket.OPEN) {
                                ws.send(JSON.stringify({
                                    type: 'js_executed',
                                    success: true,
                                    timestamp: new Date().toISOString(),
                                    message: 'JavaScript executed successfully on client',
                                    output: capturedOutput,
                                    result: result !== undefined ? String(result) : undefined,
                                    url: window.location.href,
                                    userAgent: navigator.userAgent,
                                    executionId: data.data.executionId
                                }));
                                console.log('🔥 CLIENT: Sent execution result and output to server');
                            }
                        }).catch(error => {
                            // Restore console methods in case of error
                            console.log = originalLog;
                            console.error = originalError;
                            console.warn = originalWarn;
                            
                            console.error('🔥 CLIENT: Async JavaScript execution failed:', error);
                            console.error('🔥 CLIENT: Error stack:', error.stack);
                            
                            // Send error back to server
                            if (ws && ws.readyState === WebSocket.OPEN) {
                                ws.send(JSON.stringify({
                                    type: 'js_executed',
                                    success: false,
                                    error: error.message,
                                    stack: error.stack,
                                    timestamp: new Date().toISOString(),
                                    output: capturedOutput,
                                    url: window.location.href,
                                    userAgent: navigator.userAgent,
                                    executionId: data.data.executionId
                                }));
                            }
                        });
                    } else {
                        console.error('🔥 CLIENT: No command found in execute_js message');
                    }
                } catch (error) {
                    console.error('🔥 CLIENT: JavaScript execution failed:', error);
                    console.error('🔥 CLIENT: Error stack:', error.stack);
                    
                    // Restore console methods in case of error
                    console.log = originalLog;
                    console.error = originalError;
                    console.warn = originalWarn;
                    
                    // Send error back to server
                    if (ws && ws.readyState === WebSocket.OPEN) {
                        ws.send(JSON.stringify({
                            type: 'js_executed',
                            success: false,
                            error: error.message,
                            stack: error.stack,
                            timestamp: new Date().toISOString(),
                            output: capturedOutput,
                            url: window.location.href,
                            userAgent: navigator.userAgent,
                            executionId: data.data.executionId
                        }));
                    }
                }
                return;
            }
            
            // Handle tab management - DISABLED TO PREVENT ENDLESS RELOAD LOOP
            if (data.type === 'tabRefresh') {
                console.log('🔄 Server requesting tab refresh - IGNORING to prevent endless loop');
                addSystemMessage('⚠️ Tab refresh request ignored (preventing reload loop)');
                // REMOVED: window.location.reload() - was causing endless reloads
                return;
            }
            
            if (data.type === 'tabFocus') {
                // Throttle tab focus spam - only log once per 30 seconds
                const now = Date.now();
                if (!window.lastTabFocusLog || (now - window.lastTabFocusLog) > 30000) {
                    console.log('🎯 Server requesting tab focus');
                    window.lastTabFocusLog = now;
                }
                
                // Multiple strategies to bring window to focus
                try {
                    // Strategy 1: Basic window focus
                    window.focus();
                    
                    // Strategy 2: Click simulation to trigger user activation
                    if (document.hasFocus && !document.hasFocus()) {
                        // Create a temporary click event to trigger user interaction
                        const clickEvent = new MouseEvent('click', {
                            view: window,
                            bubbles: true,
                            cancelable: true
                        });
                        document.body.dispatchEvent(clickEvent);
                        window.focus();
                    }
                    
                    // Strategy 3: Modern focus with user activation
                    if (navigator.userActivation && !navigator.userActivation.isActive) {
                        // Request user activation and focus
                        setTimeout(() => {
                            window.focus();
                        }, 100);
                    }
                    
                    // Strategy 4: Audio context trick (if available)
                    if (window.AudioContext || window.webkitAudioContext) {
                        try {
                            const audioContext = new (window.AudioContext || window.webkitAudioContext)();
                            if (audioContext.state === 'suspended') {
                                audioContext.resume().then(() => {
                                    window.focus();
                                });
                            }
                        } catch (e) {
                            // Audio context not available, continue
                        }
                    }
                    
                    // Throttle tab active message - only show once per 30 seconds
                    if (!window.lastTabActiveMsg || (now - window.lastTabActiveMsg) > 30000) {
                        addSystemMessage('📍 This tab is now active. Please use this window.');
                        window.lastTabActiveMsg = now;
                    }
                    
                } catch (error) {
                    console.warn('Focus strategies failed:', error);
                    // Only show error message once per 30 seconds
                    if (!window.lastTabErrorMsg || (now - window.lastTabErrorMsg) > 30000) {
                        addSystemMessage('⚠️ Please click on this tab to activate it.');
                        window.lastTabErrorMsg = now;
                    }
                }
                
                return;
            }
            
            if (data.type === 'closeTab') {
                console.log('🚪 Server requesting tab close:', data.message);
                addSystemMessage(data.message);
                
                // Close this tab after a brief delay
                setTimeout(() => {
                    window.close();
                }, 1000);
                return;
            }
            
            if (data.type === 'systemMessage') {
                console.log('📢 System message:', data.message);
                addSystemMessage(data.message);
                return;
            }
            
            if (data.type === 'response') {
                addMessage(data.agent || 'AI Agent', data.message, 'ai');
                updateCosts(data.cost, data.requests);
                
                // Check if this is a response to a retrain consultation
                if (window.pendingRetrain && (
                    data.message.includes('specialization') || 
                    data.message.includes('training') || 
                    data.message.includes('approach') ||
                    data.message.includes('Enhancement') ||
                    data.message.includes('EnhancedSystemAI') ||
                    (Date.now() - window.pendingRetrain.timestamp) < 60000 // Within 1 minute
                )) {
                    showRetrainConfirmation(data.message);
                }
            } else if (data.type === 'academy_update') {
                handleAcademyUpdate(data);
            } else if (data.type === 'academy_status_push') {
                // INTERRUPT-DRIVEN: Backend pushes status when it changes
                console.log('🎓 Frontend: PUSH received - Active:', data.status?.activeTraining?.length || 0, 'Completed:', data.status?.completed?.length || 0);
                
                // 1. Update stats numbers immediately (fast)
                updateAcademyStatsOnly(data.status);
                
                // 2. Update all Academy widgets dynamically (full experience)
                updateAcademyWidgets(data.status);
            } else if (data.type === 'academy_status') {
                // OLD POLLING METHOD - ignore these
                console.log('🎓 Frontend: Ignoring old polling response');
            } else if (data.type === 'agents_update') {
                // Handle agent list updates (humans and AIs)
                console.log('🤖 Received agents update:', data.agents);
                updateAvailableAgents(data.agents);
            }
        }
        
        function showRetrainConfirmation(aiResponse) {
            if (!window.pendingRetrain) return;
            
            const { personaName, improvements } = window.pendingRetrain;
            
            // Create a floating confirmation dialog
            const confirmDialog = document.createElement('div');
            confirmDialog.style.cssText = \`
                position: fixed;
                top: 50%;
                left: 50%;
                transform: translate(-50%, -50%);
                background: linear-gradient(135deg, #1a1a2e 0%, #2a2a3e 100%);
                border: 2px solid #00d4ff;
                border-radius: 12px;
                padding: 20px;
                max-width: 500px;
                z-index: 1000;
                box-shadow: 0 0 30px rgba(0, 212, 255, 0.4);
            \`;
            
            confirmDialog.innerHTML = \`
                <div style="color: #00d4ff; font-size: 16px; font-weight: bold; margin-bottom: 12px;">
                    🎓 Academy Training Recommendation for \${personaName}
                </div>
                <div style="color: #ccc; font-size: 13px; margin-bottom: 15px; line-height: 1.4;">
                    <strong>Your Request:</strong> \${improvements}
                </div>
                <div style="color: #aaa; font-size: 12px; margin-bottom: 20px; line-height: 1.4; background: rgba(0,0,0,0.3); padding: 10px; border-radius: 6px;">
                    <strong>AI Recommendation:</strong><br>\${aiResponse}
                </div>
                <div style="display: flex; gap: 10px; justify-content: flex-end;">
                    <button onclick="cancelRetrain()" style="
                        background: rgba(244, 67, 54, 0.3);
                        color: #F44336;
                        border: 1px solid #F44336;
                        padding: 8px 16px;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 12px;
                    ">Cancel</button>
                    <button onclick="confirmRetrain()" style="
                        background: linear-gradient(135deg, #FF9800 0%, #F57C00 100%);
                        color: white;
                        border: 1px solid #FF9800;
                        padding: 8px 16px;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 12px;
                        font-weight: bold;
                    ">🎓 Start Training</button>
                </div>
            \`;
            
            document.body.appendChild(confirmDialog);
            window.activeDialog = confirmDialog;
        }
        
        function confirmRetrain() {
            if (window.activeDialog) {
                document.body.removeChild(window.activeDialog);
                window.activeDialog = null;
            }
            
            // Extract specialization from AI response (simple heuristic)
            const specialization = 'ai_recommended_enhancement';
            const description = 'Custom training based on AI analysis and user requirements';
            
            startCustomTraining(specialization, description);
        }
        
        function cancelRetrain() {
            if (window.activeDialog) {
                document.body.removeChild(window.activeDialog);
                window.activeDialog = null;
            }
            window.pendingRetrain = null;
            addSystemMessage('🚫 Retrain cancelled');
        }
        
        let currentOpenAgent = null;
        
        function toggleAgentInfo(agentName) {
            const panel = document.getElementById('agent-details-panel');
            
            // If clicking the same agent that's open, close it
            if (currentOpenAgent === agentName && panel.classList.contains('open')) {
                closeAgentDetails();
                return;
            }
            
            // If another agent is open, close it fast first
            if (currentOpenAgent && currentOpenAgent !== agentName && panel.classList.contains('open')) {
                panel.classList.add('fast-close');
                panel.classList.remove('open');
                
                // Wait for fast close, then open new one
                setTimeout(() => {
                    panel.classList.remove('fast-close');
                    openAgentDetails(agentName);
                }, 150);
                return;
            }
            
            // Normal open
            openAgentDetails(agentName);
        }
        
        function openAgentDetails(agentName) {
            const panel = document.getElementById('agent-details-panel');
            
            // Update button states
            document.querySelectorAll('.agent-dropdown-btn').forEach(btn => {
                btn.classList.remove('active');
            });
            
            const activeBtn = document.querySelector('#agent-' + agentName + ' .agent-dropdown-btn');
            if (activeBtn) {
                activeBtn.classList.add('active');
            }
            
            currentOpenAgent = agentName;
            
            // Agent data
            const agentData = {
                'PlannerAI': {
                    avatar: '📋',
                    specialization: 'Strategic planning & task orchestration',
                    accuracy: '91.8%',
                    responseTime: '320ms avg',
                    tools: 'Web commands, file operations, analysis',
                    lastTraining: '1 day ago',
                    strength: 'Complex multi-step task planning',
                    model: 'Claude-3 Haiku',
                    status: 'Online',
                    languages: 'Natural language processing',
                    webAccess: 'Full web browsing & fetching',
                    fileOps: 'Read, write, analyze files',
                    advanced: 'Multi-agent coordination'
                },
                'CodeAI': {
                    avatar: '💻',
                    specialization: 'Code analysis, debugging & optimization',
                    accuracy: '96.5%',
                    responseTime: '240ms avg',
                    tools: 'JavaScript, Python, TypeScript, Rust',
                    lastTraining: '3 days ago',
                    strength: 'Complex code pattern recognition',
                    model: 'Claude-3 Sonnet',
                    status: 'Online',
                    languages: 'JavaScript, Python, TypeScript, Rust, Go',
                    webAccess: 'Documentation & Stack Overflow',
                    fileOps: 'Code analysis & refactoring',
                    advanced: 'Performance optimization'
                },
                'GeneralAI': {
                    avatar: '💬',
                    specialization: 'General assistance & conversation',
                    accuracy: '88.3%',
                    responseTime: '180ms avg',
                    tools: 'General knowledge, basic analysis',
                    lastTraining: '2 days ago',
                    strength: 'Natural conversation & explanations',
                    model: 'Claude-3 Haiku',
                    status: 'Online',
                    languages: 'Multilingual support',
                    webAccess: 'Basic web information',
                    fileOps: 'Basic file reading',
                    advanced: 'Context awareness'
                },
                'ProtocolSheriff': {
                    avatar: '⚠️',
                    specialization: 'Response validation & quality control',
                    accuracy: '99.1%',
                    responseTime: '150ms avg',
                    tools: 'Response validation, garbage detection',
                    lastTraining: '6 hours ago',
                    strength: 'Quality assurance & validation',
                    model: 'Claude-3 Haiku',
                    status: 'Online',
                    languages: 'Natural language validation',
                    webAccess: 'None (security focused)',
                    fileOps: 'Log analysis only',
                    advanced: 'Real-time quality monitoring'
                }
            };
            
            const data = agentData[agentName];
            if (!data) return;
            
            // Update panel content
            document.getElementById('agent-details-avatar').textContent = data.avatar;
            document.getElementById('agent-details-name').textContent = agentName;
            document.getElementById('detail-specialization').textContent = data.specialization;
            document.getElementById('detail-accuracy').textContent = data.accuracy;
            document.getElementById('detail-response-time').textContent = data.responseTime;
            document.getElementById('detail-tools').textContent = data.tools;
            document.getElementById('detail-last-training').textContent = data.lastTraining;
            document.getElementById('detail-strength').textContent = data.strength;
            document.getElementById('detail-model').textContent = data.model;
            document.getElementById('detail-status').textContent = data.status;
            document.getElementById('detail-languages').textContent = data.languages;
            document.getElementById('detail-web-access').textContent = data.webAccess;
            document.getElementById('detail-file-ops').textContent = data.fileOps;
            document.getElementById('detail-advanced').textContent = data.advanced;
            
            // Open the panel
            panel.classList.add('open');
        }
        
        function closeAgentDetails() {
            const panel = document.getElementById('agent-details-panel');
            panel.classList.remove('open');
            
            // Clear button states
            document.querySelectorAll('.agent-dropdown-btn').forEach(btn => {
                btn.classList.remove('active');
            });
            
            currentOpenAgent = null;
        }
        
        // Close panel when clicking outside
        document.addEventListener('click', function(event) {
            const panel = document.getElementById('agent-details-panel');
            const isClickInsidePanel = panel?.contains(event.target);
            const isDropdownButton = event.target?.classList?.contains('agent-dropdown-btn');
            
            if (panel?.classList.contains('open') && !isClickInsidePanel && !isDropdownButton) {
                closeAgentDetails();
            }
        });
        
        function retrainAgent(agentName) {
            // Use the same retrain flow as personas
            retrainPersona(agentName, agentName);
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
            
            // Update academy section with real-time data
            updateAcademySection({ activeTraining: [session], completed: [] });
        }
        
        let lastAcademyUpdate = 0;
        function updateAcademySection(data) {
            // Light throttling for push updates (once per 500ms)
            const now = Date.now();
            if (now - lastAcademyUpdate < 500) {
                console.log('🎓 Frontend: Academy update throttled (500ms)');
                return;
            }
            lastAcademyUpdate = now;
            
            console.log('🎓 Frontend: updateAcademySection called');
            // Handle nested status object
            const status = data.status || data;
            const { activeTraining = [], completed = [] } = status;
            console.log('🎓 Frontend: Active training:', activeTraining.length, 'Completed:', completed.length);
            
            // Update Academy sidebar training list
            updateAcademySidebar(activeTraining, completed);
            
            // Update the main academy content area with cyberpunk interface
            const academyContent = document.getElementById('academy-content');
            if (!academyContent) return;
            
            let academyHTML = \`
                <div class="academy-cyber-training" style="padding: 0; max-height: calc(100vh - 140px); overflow-y: auto;">
                    <!-- Cyberpunk Academy Header -->
                    <div style="
                        background: linear-gradient(135deg, #0a0a0a 0%, #1a1a2e 100%); 
                        border: 1px solid #00d4ff; 
                        border-radius: 10px; 
                        padding: 15px 18px; 
                        margin-bottom: 18px; 
                        box-shadow: 0 0 15px rgba(0, 212, 255, 0.2);
                        position: relative;
                        overflow: hidden;
                    ">
                        <div style="position: absolute; top: 0; left: 0; right: 0; height: 1px; background: linear-gradient(90deg, transparent, #00d4ff, transparent); opacity: 0.7;"></div>
                        <div style="color: #00d4ff; font-size: 15px; font-weight: bold; margin-bottom: 4px; text-shadow: 0 0 8px rgba(0, 212, 255, 0.4);">
                            🎓 Academy Training Command
                        </div>
                        <div style="color: #888; font-size: 12px;">
                            \${activeTraining.length} training • \${completed.filter(c => c.status === 'graduated').length} graduated • \${completed.filter(c => c.status === 'failed').length} failed
                        </div>
                    </div>

                    <!-- Active Training Sessions -->
                    \${activeTraining.length > 0 ? activeTraining.map(session => \`
                        <div class="cyber-training-card" style="
                            background: linear-gradient(135deg, #1a1a1a 0%, #2a2a3e 100%); 
                            border: 1px solid #7c4dff; 
                            border-radius: 10px; 
                            padding: 16px; 
                            margin-bottom: 14px; 
                            box-shadow: 0 4px 20px rgba(124, 77, 255, 0.15);
                            position: relative;
                            transition: all 0.3s ease;
                        " onmouseover="this.style.borderColor='#00d4ff'; this.style.boxShadow='0 6px 25px rgba(0, 212, 255, 0.25)'" 
                           onmouseout="this.style.borderColor='#7c4dff'; this.style.boxShadow='0 4px 20px rgba(124, 77, 255, 0.15)'">
                            
                            <!-- Glowing top border -->
                            <div style="position: absolute; top: 0; left: 0; right: 0; height: 2px; background: linear-gradient(90deg, transparent, #FF9800, transparent); opacity: 0.8;"></div>
                            
                            <!-- Agent Header -->
                            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;">
                                <h3 style="
                                    color: #ffffff; 
                                    margin: 0; 
                                    font-size: 16px; 
                                    font-weight: bold;
                                    text-decoration: underline; 
                                    cursor: pointer;
                                    text-shadow: 0 0 5px rgba(255, 255, 255, 0.3);
                                " onclick="toggleAcademyDetails('\${session.personaName}')">
                                    \${session.personaName}
                                </h3>
                                <span style="
                                    color: #FF9800; 
                                    font-size: 11px; 
                                    background: rgba(255, 152, 0, 0.15);
                                    padding: 4px 10px;
                                    border-radius: 15px;
                                    border: 1px solid rgba(255, 152, 0, 0.4);
                                    text-transform: uppercase;
                                    letter-spacing: 0.5px;
                                    font-weight: bold;
                                ">
                                    \${session.status.replace(/_/g, ' ')}
                                </span>
                            </div>
                            
                            <!-- Stats Row with Cyber Styling -->
                            <div style="display: flex; justify-content: space-between; margin-bottom: 15px;">
                                <div style="
                                    background: rgba(0, 212, 255, 0.1); 
                                    border: 1px solid rgba(0, 212, 255, 0.3); 
                                    border-radius: 8px; 
                                    padding: 8px 12px; 
                                    text-align: center;
                                    flex: 1;
                                    margin-right: 8px;
                                ">
                                    <div style="color: #00d4ff; font-size: 14px; font-weight: bold;">
                                        \${session.currentRound || 0}/\${session.totalRounds || 10}
                                    </div>
                                    <div style="color: #888; font-size: 10px; text-transform: uppercase;">Rounds</div>
                                </div>
                                <div style="
                                    background: rgba(76, 175, 80, 0.1); 
                                    border: 1px solid rgba(76, 175, 80, 0.3); 
                                    border-radius: 8px; 
                                    padding: 8px 12px; 
                                    text-align: center;
                                    flex: 1;
                                    margin-left: 8px;
                                ">
                                    <div style="color: #4caf50; font-size: 14px; font-weight: bold;">
                                        \${((session.graduationScore || 0) * 100).toFixed(1)}%
                                    </div>
                                    <div style="color: #888; font-size: 10px; text-transform: uppercase;">Accuracy</div>
                                </div>
                            </div>
                            
                            <!-- Specialization & Time -->
                            <div style="
                                background: rgba(0, 0, 0, 0.3); 
                                border-radius: 6px; 
                                padding: 10px; 
                                margin-bottom: 12px;
                                border: 1px solid #333;
                            ">
                                <div style="font-size: 12px; color: #ccc; margin-bottom: 4px;">
                                    <span style="color: #7c4dff; font-weight: bold;">Specialization:</span> \${session.specialization.replace(/_/g, ' ')}
                                </div>
                                <div style="font-size: 11px; color: #888;">
                                    <span style="color: #7c4dff;">Started:</span> \${session.startTime ? new Date(session.startTime).toLocaleTimeString() : 'Unknown'}
                                </div>
                            </div>
                            
                            <!-- Recent Activity Terminal -->
                            \${session.logs && session.logs.length > 0 ? \`
                                <div style="
                                    background: rgba(0, 0, 0, 0.7); 
                                    border: 1px solid #00d4ff; 
                                    border-radius: 6px; 
                                    padding: 10px; 
                                    max-height: 70px; 
                                    overflow-y: auto;
                                    font-family: 'Courier New', monospace;
                                ">
                                    <div style="color: #00d4ff; font-size: 9px; margin-bottom: 6px; text-transform: uppercase; letter-spacing: 1px;">
                                        > Recent Activity
                                    </div>
                                    \${session.logs.slice(-2).map(log => \`
                                        <div style="color: #aaa; font-size: 10px; margin: 2px 0; line-height: 1.3;">
                                            <span style="color: #00d4ff;">></span> \${log}
                                        </div>
                                    \`).join('')}
                                </div>
                            \` : ''}
                        </div>
                    \`).join('') : \`
                        <div style="
                            background: linear-gradient(135deg, #0f0f0f 0%, #1a1a2e 100%);
                            border: 1px dashed #555;
                            border-radius: 10px;
                            padding: 30px 20px;
                            text-align: center;
                            margin-bottom: 18px;
                        ">
                            <div style="color: #7c4dff; font-size: 36px; margin-bottom: 12px;">🎓</div>
                            <div style="color: #888; font-size: 14px; font-weight: bold;">No Active Training Sessions</div>
                            <div style="color: #666; font-size: 11px; margin-top: 6px;">
                                Deploy agents to the Academy for specialized training
                            </div>
                        </div>
                    \`}

                    <!-- Recently Completed -->
                    \${completed.length > 0 ? \`
                        <div style="margin-top: 18px;">
                            <h3 style="color: #7c4dff; font-size: 13px; margin-bottom: 10px; text-transform: uppercase; letter-spacing: 1px;">Recent Graduates</h3>
                            \${completed.slice(0, 3).map(session => \`
                                <div style="
                                    background: linear-gradient(135deg, #1a1a1a 0%, #2a2a3e 100%); 
                                    border: 1px solid \${session.status === 'graduated' ? '#4CAF50' : '#f44336'}; 
                                    border-radius: 8px; 
                                    padding: 12px; 
                                    margin-bottom: 8px; 
                                    box-shadow: 0 2px 10px rgba(\${session.status === 'graduated' ? '76, 175, 80' : '244, 67, 54'}, 0.1);
                                ">
                                    <div style="display: flex; justify-content: space-between; align-items: center;">
                                        <span style="color: #e0e0e0; font-weight: bold; font-size: 13px;">\${session.personaName}</span>
                                        <span style="
                                            color: \${session.status === 'graduated' ? '#4CAF50' : '#f44336'}; 
                                            font-size: 10px;
                                            background: rgba(\${session.status === 'graduated' ? '76, 175, 80' : '244, 67, 54'}, 0.15);
                                            padding: 3px 8px;
                                            border-radius: 12px;
                                            border: 1px solid rgba(\${session.status === 'graduated' ? '76, 175, 80' : '244, 67, 54'}, 0.3);
                                            text-transform: uppercase;
                                            font-weight: bold;
                                        ">
                                            \${session.status === 'graduated' ? '🎓 Graduated' : '❌ Failed'}
                                        </span>
                                    </div>
                                    <div style="font-size: 10px; color: #888; margin-top: 4px;">
                                        \${session.specialization.replace(/_/g, ' ')} • \${((session.graduationScore || 0) * 100).toFixed(1)}% accuracy
                                        \${session.fineTuneId ? ' • LoRA: ' + session.fineTuneId.substring(0, 18) + '...' : ''}
                                    </div>
                                </div>
                            \`).join('')}
                        </div>
                    \` : ''}

                    <!-- Cyberpunk Action Buttons -->
                    <div style="margin-top: 25px; display: grid; grid-template-columns: 1fr 1fr; gap: 12px;">
                        <button onclick="sendSheriffToAcademy()" style="
                            background: linear-gradient(135deg, #4caf50 0%, #2e7d32 100%); 
                            border: 1px solid #4caf50;
                            color: white; 
                            padding: 14px 16px; 
                            border-radius: 8px; 
                            cursor: pointer; 
                            font-size: 12px; 
                            font-weight: bold;
                            text-transform: uppercase;
                            letter-spacing: 0.5px;
                            transition: all 0.3s ease;
                            box-shadow: 0 4px 15px rgba(76, 175, 80, 0.2);
                        " onmouseover="this.style.transform='translateY(-1px)'; this.style.boxShadow='0 6px 20px rgba(76, 175, 80, 0.35)'" 
                           onmouseout="this.style.transform='translateY(0)'; this.style.boxShadow='0 4px 15px rgba(76, 175, 80, 0.2)'">
                            🛡️ Deploy Sheriff
                        </button>
                        <button onclick="trainCustomPersona()" style="
                            background: linear-gradient(135deg, #2196f3 0%, #1565c0 100%); 
                            border: 1px solid #2196f3;
                            color: white; 
                            padding: 14px 16px; 
                            border-radius: 8px; 
                            cursor: pointer; 
                            font-size: 12px; 
                            font-weight: bold;
                            text-transform: uppercase;
                            letter-spacing: 0.5px;
                            transition: all 0.3s ease;
                            box-shadow: 0 4px 15px rgba(33, 150, 243, 0.2);
                        " onmouseover="this.style.transform='translateY(-1px)'; this.style.boxShadow='0 6px 20px rgba(33, 150, 243, 0.35)'" 
                           onmouseout="this.style.transform='translateY(0)'; this.style.boxShadow='0 4px 15px rgba(33, 150, 243, 0.2)'">
                            🎓 Custom Training
                        </button>
                    </div>
                </div>

                <style>
                    .academy-cyber-training::-webkit-scrollbar {
                        width: 6px;
                    }
                    .academy-cyber-training::-webkit-scrollbar-track {
                        background: rgba(0, 0, 0, 0.3);
                        border-radius: 3px;
                    }
                    .academy-cyber-training::-webkit-scrollbar-thumb {
                        background: linear-gradient(135deg, #7c4dff, #00d4ff);
                        border-radius: 3px;
                    }
                </style>
            \`;
            
            academyContent.innerHTML = academyHTML;
        }
        
        function updateAcademySidebar(activeTraining, completed) {
            console.log('🎓 Frontend: updateAcademySidebar called with:', activeTraining.length, 'active,', completed.length, 'completed');
            // Update Academy training list in sidebar
            const trainingList = document.getElementById('academy-training-list');
            if (trainingList) {
                if (activeTraining.length === 0) {
                    trainingList.innerHTML = '<div class="loading">No active training sessions</div>';
                } else {
                    trainingList.innerHTML = activeTraining.map(session => \`
                        <div class="academy-training-item" onclick="focusTrainingSession('\${session.personaName}')">
                            <div class="training-header">
                                <div class="training-name">\${session.personaName}</div>
                                <div class="training-status">\${session.status.replace(/_/g, ' ')}</div>
                            </div>
                            <div class="training-progress">
                                Round \${session.currentRound || 0}/\${session.totalRounds || 10} • \${((session.graduationScore || 0) * 100).toFixed(1)}% accuracy
                            </div>
                            <div class="training-bar">
                                <div class="training-fill" style="width: \${session.progress || 0}%"></div>
                            </div>
                        </div>
                    \`).join('');
                }
            }
            
            // Update Academy statistics
            const activeCount = document.getElementById('academy-active-count');
            const graduatedCount = document.getElementById('academy-graduated-count');
            
            console.log('🎓 Frontend: Updating stats - activeCount element:', !!activeCount, 'graduatedCount element:', !!graduatedCount);
            console.log('🎓 Frontend: Document title:', document.title);
            console.log('🎓 Frontend: Current room elements in DOM:', document.querySelectorAll('[id*="academy"]').length);
            
            if (activeCount) {
                const newValue = activeTraining.length;
                console.log('🎓 Frontend: BEFORE update - activeCount.textContent:', activeCount.textContent);
                activeCount.textContent = newValue;
                console.log('🎓 Frontend: AFTER update - activeCount.textContent:', activeCount.textContent);
                console.log('🎓 Frontend: Set active count to:', newValue);
                
                // Force a visual update
                activeCount.style.color = 'red';
                setTimeout(() => activeCount.style.color = '', 100);
            } else {
                console.log('🎓 Frontend: ❌ activeCount element is NULL - element does not exist in DOM');
                console.log('🎓 Frontend: All elements with academy in ID:', Array.from(document.querySelectorAll('[id*="academy"]')).map(el => el.id));
            }
            
            if (graduatedCount) {
                const newValue = completed.filter(c => c.status === 'graduated').length;
                console.log('🎓 Frontend: BEFORE update - graduatedCount.textContent:', graduatedCount.textContent);
                graduatedCount.textContent = newValue;
                console.log('🎓 Frontend: AFTER update - graduatedCount.textContent:', graduatedCount.textContent);
                console.log('🎓 Frontend: Set graduated count to:', newValue);
                
                // Force a visual update
                graduatedCount.style.color = 'red';
                setTimeout(() => graduatedCount.style.color = '', 100);
            } else {
                console.log('🎓 Frontend: ❌ graduatedCount element is NULL - element does not exist in DOM');
            }
            
            // Update recent graduates list
            const graduatesList = document.getElementById('academy-graduates-list');
            if (graduatesList) {
                const recentGraduates = completed.filter(c => c.status === 'graduated').slice(-3);
                
                if (recentGraduates.length === 0) {
                    graduatesList.innerHTML = '<div class="loading">No recent graduates</div>';
                } else {
                    graduatesList.innerHTML = recentGraduates.map(session => \`
                        <div class="academy-graduate-item">
                            <div class="graduate-name">\${session.personaName}</div>
                            <div class="graduate-info">
                                \${session.specialization.replace(/_/g, ' ')} • \${((session.graduationScore || 0) * 100).toFixed(1)}% accuracy
                                \${session.fineTuneId ? ' • LoRA: ' + session.fineTuneId.substring(0, 12) + '...' : ''}
                            </div>
                        </div>
                    \`).join('');
                }
            }
        }
        
        function focusTrainingSession(personaName) {
            // When clicking a training session in sidebar, focus it in the main chat
            addMessage('Academy System', \`🎯 Focusing on \${personaName} training session...\`, 'ai', true);
        }
        
        // Room switching functionality
        function switchRoom(room) {
            currentRoom = room;
            
            // Save state when switching rooms
            saveChatHistory();
            
            // Update tab appearance
            document.querySelectorAll('.room-tab').forEach(tab => tab.classList.remove('active'));
            document.getElementById(\`tab-\${room}\`).classList.add('active');
            
            // Update sidebar content
            document.querySelectorAll('.room-content').forEach(content => content.style.display = 'none');
            document.getElementById(\`\${room}-room\`).style.display = 'block';
            
            // Update chat header
            if (room === 'general') {
                setChatTitle('Active Chat');
                document.getElementById('chat-subtitle').textContent = 'Talk to specialized AI agents';
            } else if (room === 'academy') {
                setChatTitle('Academy Training Room');
                document.getElementById('chat-subtitle').textContent = 'Watch AI agents train and improve their skills';
                
                // INTERRUPT-DRIVEN: Request initial Academy status only once
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({ type: 'get_initial_academy_status' }));
                }
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
                    addMessage('System', '💬 Welcome to Active Chat! Ask me anything and I\\'ll route it to the best AI agent.', 'ai');
                } else if (room === 'academy') {
                    addMessage('System', '🎓 Welcome to the Academy! Here you can watch AI agents train and see their progress in real-time.', 'ai', true);
                }
            }
        }
        
        function addMessage(sender, content, type = 'ai', isAcademy = false, skipSave = false) {
            const messageObj = { sender, content, type, time: new Date(), isAcademy };
            
            // Add to appropriate room (unless it's a temporary message)
            if (!skipSave) {
                const targetRoom = isAcademy ? 'academy' : currentRoom;
                if (!roomMessages[targetRoom]) roomMessages[targetRoom] = [];
                roomMessages[targetRoom].push(messageObj);
                
                // Update message history for API
                messageHistory.push(messageObj);
            }
            
            // Always show if we're in the target room
            if (currentRoom === (isAcademy ? 'academy' : currentRoom)) {
                addMessageToChat(sender, content, type, isAcademy);
            }
            
            // Save to localStorage (unless it's a temporary message)
            if (!skipSave) {
                saveChatHistory();
            }
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
            // Clean up the agent ID if it has whitespace
            agentId = agentId.toString().trim();
            
            if (isGroupChat) {
                // Multi-select mode for group chat
                if (selectedAgents.has(agentId)) {
                    selectedAgents.delete(agentId);
                    const element = document.getElementById(\`agent-\${agentId}\`);
                    if (element) element.classList.remove('selected');
                } else {
                    selectedAgents.add(agentId);
                    const element = document.getElementById(\`agent-\${agentId}\`);
                    if (element) element.classList.add('selected');
                }
                updateGroupChatStatus();
            } else {
                // Single select mode
                selectedAgent = agentId;
                document.querySelectorAll('.agent-item').forEach(item => item.classList.remove('selected'));
                const element = document.getElementById(\`agent-\${agentId}\`);
                if (element) {
                    element.classList.add('selected');
                } else {
                    console.warn('Agent element not found:', \`agent-\${agentId}\`);
                }
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
            const subtitle = document.getElementById('chat-subtitle');
            
            if (currentRoom === 'general') {
                if (isGroupChat && selectedAgents.size > 0) {
                    setChatTitle(\`Group Chat (\${selectedAgents.size} agents)\`);
                    subtitle.textContent = \`Talking to: \${Array.from(selectedAgents).join(', ')}\`;
                } else if (selectedAgent !== 'auto') {
                    setChatTitle(\`Chat with \${selectedAgent}\`);
                    subtitle.textContent = \`Direct conversation with \${selectedAgent}\`;
                } else {
                    setChatTitle('General Chat');
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
                    <button class="persona-action-btn retrain" onclick="retrainPersona('\${persona.id}', '\${persona.name}')">
                        🎓 Retrain
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
        
        async function retrainPersona(personaId, personaName) {
            try {
                // First, get user input about desired improvements
                const improvements = prompt(\`What improvements would you like for \${personaName}?\\n\\nExamples:\\n- Better code generation\\n- More detailed explanations\\n- Faster responses\\n- Better error handling\\n- More creative solutions\`);
                
                if (!improvements) {
                    return; // User cancelled
                }
                
                // Ask our AI to help design the training based on the improvements
                addSystemMessage(\`🤖 Analyzing improvement request for \${personaName}...\`);
                
                if (ws && ws.readyState === WebSocket.OPEN) {
                    // Send message to AI to get training recommendations
                    ws.send(JSON.stringify({
                        type: 'message',
                        content: \`I want to retrain the AI persona "\${personaName}" with these improvements: "\${improvements}". Please suggest a specific specialization name and training approach for Academy training that would address these improvements. Keep your response concise - just the specialization name and brief description.\`,
                        room: 'general'
                    }));
                    
                    // Store the pending retrain request
                    window.pendingRetrain = {
                        personaId,
                        personaName,
                        improvements,
                        timestamp: Date.now()
                    };
                    
                    // Show the pending training button in Academy
                    showPendingTrainingButton();
                    
                    addSystemMessage(\`🎓 Getting AI recommendations for \${personaName} improvements...\\nOnce ready, we'll start Academy training with a custom specialization.\`);
                    
                    // Also show a message in Academy room
                    addMessage('Academy System', \`🤖 \${personaName} retrain analysis in progress... Will start Academy training once AI provides recommendations.\`, 'ai', true);
                }
                
            } catch (error) {
                console.error('Retrain persona error:', error);
                addSystemMessage(\`❌ Failed to initiate retrain: \${error.message}\`);
            }
        }
        
        // Function to actually start the training after AI consultation
        function startCustomTraining(specialization, description) {
            if (!window.pendingRetrain) return;
            
            const { personaName, improvements } = window.pendingRetrain;
            
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'start_academy_training',
                    personaName: personaName + '-enhanced-' + Date.now(),
                    specialization: specialization || 'custom_enhancement',
                    rounds: 15,
                    customPrompt: \`Training Focus: \${improvements}\\n\\nSpecialization: \${specialization}\\n\\nDescription: \${description}\`,
                    trainingIntensity: 'high'
                }));
                
                addSystemMessage(\`🎓 \${personaName} sent to Academy for specialized training: \${specialization}\`);
                
                // Switch to Academy room to watch the training
                switchRoom('academy');
                
                // Clear pending retrain
                window.pendingRetrain = null;
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
            // Show advanced training dialog for Sheriff training
            showAdvancedTrainingDialog('Sheriff-' + Date.now(), 'protocol_enforcement');
        }
        
        function trainCustomPersona() {
            const personaName = prompt('Enter persona name:');
            const specialization = prompt('Enter specialization:') || 'protocol_enforcement';
            
            if (personaName) {
                // Show advanced training dialog for custom persona
                showAdvancedTrainingDialog(personaName, specialization);
            }
        }
        
        // DIRECT Academy stats update function (called by push events)
        function updateAcademyStatsOnly(status) {
            const { activeTraining = [], completed = [] } = status;
            
            console.log('🎓 Frontend: updateAcademyStatsOnly - updating numbers only');
            
            // Update Academy statistics numbers immediately
            const activeCount = document.getElementById('academy-active-count');
            const graduatedCount = document.getElementById('academy-graduated-count');
            
            if (activeCount) {
                activeCount.textContent = activeTraining.length;
                activeCount.style.color = '#00d4ff';
                setTimeout(() => activeCount.style.color = '', 200);
                console.log('🎓 Stats: Active count updated to:', activeTraining.length);
            }
            
            if (graduatedCount) {
                const graduatedNum = completed.filter(c => c.status === 'graduated').length;
                graduatedCount.textContent = graduatedNum;
                graduatedCount.style.color = '#4CAF50';
                setTimeout(() => graduatedCount.style.color = '', 200);
                console.log('🎓 Stats: Graduated count updated to:', graduatedNum);
            }
        }
        
        // DYNAMIC Academy widgets update (handles training cards, progress bars, moving between sections)
        function updateAcademyWidgets(status) {
            const { activeTraining = [], completed = [] } = status;
            
            console.log('🎓 Frontend: updateAcademyWidgets - updating all dynamic content');
            
            // Update Training Status section in sidebar
            updateTrainingStatusWidget(activeTraining);
            
            // Update Recent Graduates section
            updateRecentGraduatesWidget(completed);
            
            // Update main Academy content area (if visible)
            updateAcademyMainContent(activeTraining, completed);
        }
        
        function updateTrainingStatusWidget(activeTraining) {
            const trainingStatus = document.querySelector('#academy-training-list');
            if (!trainingStatus) return;
            
            console.log('🎓 Updating Training Status widget');
            
            if (activeTraining.length === 0) {
                trainingStatus.innerHTML = \`
                    <div class="loading">No active training sessions</div>
                \`;
            } else {
                trainingStatus.innerHTML = \`
                    \${activeTraining.map((session, index) => \`
                        <div class="training-card" style="
                            background: linear-gradient(135deg, #1a1a1a 0%, #2a2a3e 100%); 
                            border: 1px solid #00d4ff; 
                            border-radius: 8px; 
                            padding: 12px; 
                            margin-bottom: 10px; 
                            box-shadow: 0 0 15px rgba(0, 212, 255, 0.2);
                            animation: fadeIn 0.3s ease-in;
                            cursor: pointer;
                        " onclick="toggleTrainingDetails('train-\${index}')">
                            <div style="color: #00d4ff; font-weight: bold; margin-bottom: 6px; display: flex; justify-content: space-between; align-items: center;">
                                <span>\${session.personaName}</span>
                                <span style="font-size: 10px; color: #888;">\${session.status.replace(/_/g, ' ')}</span>
                            </div>
                            <div style="color: #888; font-size: 11px; margin-bottom: 8px;">
                                \${session.specialization.replace(/_/g, ' ')} • Round \${session.currentRound || 0}/\${session.totalRounds || 15}
                            </div>
                            <div class="progress-container" style="
                                background: #0a0a0a; 
                                border-radius: 6px; 
                                height: 6px; 
                                margin-bottom: 6px;
                                overflow: hidden;
                            ">
                                <div class="progress-bar" style="
                                    background: linear-gradient(90deg, #00d4ff, #0099cc); 
                                    height: 100%; 
                                    width: \${session.progress || 0}%; 
                                    transition: width 0.5s ease;
                                    box-shadow: 0 0 8px rgba(0, 212, 255, 0.5);
                                "></div>
                            </div>
                            <div style="color: #aaa; font-size: 10px; display: flex; justify-content: space-between;">
                                <span>\${session.progress || 0}% Complete</span>
                                <span>\${((session.graduationScore || 0) * 100).toFixed(1)}% Accuracy</span>
                            </div>
                            
                            <!-- Expandable Details -->
                            <div id="train-\${index}" class="training-details" style="display: none; margin-top: 10px; padding-top: 10px; border-top: 1px solid #333;">
                                <div style="color: #aaa; font-size: 10px; margin-bottom: 6px;">📊 Live Statistics:</div>
                                <div style="color: #888; font-size: 10px; line-height: 1.4;">
                                    • Started: \${new Date(session.startTime).toLocaleTimeString()}<br>
                                    • Elapsed: \${Math.round((Date.now() - new Date(session.startTime)) / 1000)}s<br>
                                    • Training Type: \${session.customPrompt ? 'Custom' : 'Standard'}<br>
                                    • Intensity: \${session.trainingIntensity || 'Normal'}<br>
                                    • Current Accuracy: \${((session.graduationScore || 0) * 100).toFixed(1)}%<br>
                                    • Target: 85% (Gap: \${Math.max(0, 85 - (session.graduationScore || 0) * 100).toFixed(1)}%)
                                </div>
                                \${session.logs && session.logs.length > 0 ? \`
                                    <div style="color: #aaa; font-size: 10px; margin-top: 6px; margin-bottom: 4px;">📋 Live Activity Feed:</div>
                                    <div style="max-height: 80px; overflow-y: auto; font-size: 9px; color: #666; background: #0a0a0a; padding: 4px; border-radius: 4px;">
                                        \${session.logs.slice(-5).map(log => \`<div style="margin: 1px 0;">• \${log}</div>\`).join('')}
                                    </div>
                                \` : ''}
                                \${session.customPrompt ? \`
                                    <div style="color: #aaa; font-size: 10px; margin-top: 6px; margin-bottom: 4px;">🎯 Training Focus:</div>
                                    <div style="font-size: 9px; color: #666; background: #0a0a0a; padding: 4px; border-radius: 4px; max-height: 40px; overflow-y: auto;">
                                        \${session.customPrompt.split('\\n')[0].substring(0, 120)}...
                                    </div>
                                \` : ''}
                            </div>
                        </div>
                    \`).join('')}
                \`;
            }
        }
        
        function updateRecentGraduatesWidget(completed) {
            const graduatesList = document.getElementById('academy-graduates-list');
            if (!graduatesList) return;
            
            console.log('🎓 Updating Recent Graduates widget');
            
            const recentGraduates = completed.filter(c => c.status === 'graduated').slice(-3);
            const recentFailed = completed.filter(c => c.status === 'failed').slice(-2);
            
            if (recentGraduates.length === 0 && recentFailed.length === 0) {
                graduatesList.innerHTML = '<div class="loading">No recent graduates</div>';
            } else {
                graduatesList.innerHTML = \`
                    \${recentGraduates.map((session, index) => \`
                        <div class="academy-graduate-item" style="
                            background: linear-gradient(135deg, #1a2a1a 0%, #2a3e2a 100%);
                            border: 1px solid #4CAF50;
                            border-radius: 6px;
                            padding: 10px;
                            margin-bottom: 8px;
                            animation: slideIn 0.4s ease-out;
                            cursor: pointer;
                        " onclick="toggleGraduateDetails('grad-\${index}')">
                            <div class="graduate-name" style="color: #4CAF50; font-weight: bold;">
                                🎓 \${session.personaName}
                            </div>
                            <div class="graduate-info" style="color: #888; font-size: 11px; margin-top: 4px;">
                                \${session.specialization.replace(/_/g, ' ')} • \${((session.graduationScore || 0) * 100).toFixed(1)}% accuracy
                                \${session.fineTuneId ? ' • LoRA: ' + session.fineTuneId.substring(0, 12) + '...' : ''}
                            </div>
                            <div id="grad-\${index}" class="graduate-details" style="display: none; margin-top: 8px; padding-top: 8px; border-top: 1px solid #333;">
                                <div style="color: #aaa; font-size: 10px; margin-bottom: 4px;">📊 Training Statistics:</div>
                                <div style="color: #888; font-size: 10px;">
                                    • Duration: \${session.completedAt ? Math.round((new Date(session.completedAt) - new Date(session.startTime)) / 1000) + 's' : 'N/A'}<br>
                                    • Rounds: \${session.totalRounds || 'N/A'}<br>
                                    • Final Accuracy: \${((session.graduationScore || 0) * 100).toFixed(1)}%<br>
                                    • Storage Reduction: \${session.storageReduction ? Math.round(session.storageReduction).toLocaleString() + 'x' : 'N/A'}<br>
                                    \${session.customPrompt ? '• Custom Training: Yes' : '• Standard Training'}
                                </div>
                                \${session.logs && session.logs.length > 0 ? \`
                                    <div style="color: #aaa; font-size: 10px; margin-top: 4px; margin-bottom: 2px;">📋 Recent Logs:</div>
                                    <div style="max-height: 60px; overflow-y: auto; font-size: 9px; color: #666;">
                                        \${session.logs.slice(-3).map(log => \`<div>• \${log}</div>\`).join('')}
                                    </div>
                                \` : ''}
                            </div>
                        </div>
                    \`).join('')}
                    \${recentFailed.map((session, index) => \`
                        <div class="academy-graduate-item" style="
                            background: linear-gradient(135deg, #2a1a1a 0%, #3e2a2a 100%);
                            border: 1px solid #f44336;
                            border-radius: 6px;
                            padding: 10px;
                            margin-bottom: 8px;
                            animation: slideIn 0.4s ease-out;
                            cursor: pointer;
                        " onclick="toggleGraduateDetails('fail-\${index}')">
                            <div class="graduate-name" style="color: #f44336; font-weight: bold;">
                                ❌ \${session.personaName}
                            </div>
                            <div class="graduate-info" style="color: #888; font-size: 11px; margin-top: 4px;">
                                Failed: \${((session.graduationScore || 0) * 100).toFixed(1)}% accuracy (needed 85%)
                            </div>
                            <div id="fail-\${index}" class="graduate-details" style="display: none; margin-top: 8px; padding-top: 8px; border-top: 1px solid #333;">
                                <div style="color: #aaa; font-size: 10px; margin-bottom: 4px;">📊 Failure Analysis:</div>
                                <div style="color: #888; font-size: 10px;">
                                    • Duration: \${session.completedAt ? Math.round((new Date(session.completedAt) - new Date(session.startTime)) / 1000) + 's' : 'N/A'}<br>
                                    • Rounds Completed: \${session.currentRound || 0}/\${session.totalRounds || 'N/A'}<br>
                                    • Peak Accuracy: \${((session.graduationScore || 0) * 100).toFixed(1)}%<br>
                                    • Gap to Pass: \${(85 - (session.graduationScore || 0) * 100).toFixed(1)}% points<br>
                                    \${session.customPrompt ? '• Custom Training: Yes' : '• Standard Training'}
                                </div>
                                \${session.logs && session.logs.length > 0 ? \`
                                    <div style="color: #aaa; font-size: 10px; margin-top: 4px; margin-bottom: 2px;">📋 Training Logs:</div>
                                    <div style="max-height: 60px; overflow-y: auto; font-size: 9px; color: #666;">
                                        \${session.logs.slice(-3).map(log => \`<div>• \${log}</div>\`).join('')}
                                    </div>
                                \` : ''}
                                <div style="margin-top: 6px;">
                                    <button onclick="event.stopPropagation(); retryTraining('\${session.personaName}')" style="
                                        background: #f44336; 
                                        color: white; 
                                        border: none; 
                                        padding: 4px 8px; 
                                        border-radius: 4px; 
                                        font-size: 9px; 
                                        cursor: pointer;
                                    ">🔄 Retry Training</button>
                                </div>
                            </div>
                        </div>
                    \`).join('')}
                \`;
            }
        }
        
        function updateAcademyMainContent(activeTraining, completed) {
            // Only update if we're currently viewing Academy
            if (currentRoom !== 'academy') return;
            
            const academyContent = document.getElementById('academy-content');
            if (!academyContent) return;
            
            console.log('🎓 Updating Academy main content area');
            
            // Update the main Academy interface with real-time training data
            // This will show the full cyberpunk training interface
            academyContent.innerHTML = \`
                <div class="academy-cyber-training" style="padding: 0; max-height: calc(100vh - 140px); overflow-y: auto;">
                    <!-- Real-time Active Training -->
                    \${activeTraining.length > 0 ? \`
                        <div style="margin-bottom: 20px;">
                            <h3 style="color: #00d4ff; margin-bottom: 15px;">🎯 Active Training Sessions</h3>
                            \${activeTraining.map(session => \`
                                <div class="cyber-training-card" style="
                                    background: linear-gradient(135deg, #0a0a0a 0%, #1a1a2e 100%); 
                                    border: 1px solid #00d4ff; 
                                    border-radius: 10px; 
                                    padding: 18px; 
                                    margin-bottom: 15px; 
                                    box-shadow: 0 0 20px rgba(0, 212, 255, 0.3);
                                    animation: pulseGlow 2s infinite alternate;
                                ">
                                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;">
                                        <div style="color: #00d4ff; font-size: 16px; font-weight: bold;">
                                            \${session.personaName}
                                        </div>
                                        <div style="color: #888; font-size: 12px;">
                                            \${session.status.replace(/_/g, ' ')}
                                        </div>
                                    </div>
                                    
                                    <div style="margin-bottom: 12px;">
                                        <div style="color: #aaa; font-size: 12px; margin-bottom: 6px;">
                                            Round \${session.currentRound || 0}/\${session.totalRounds || 15} • \${session.specialization.replace(/_/g, ' ')}
                                        </div>
                                        <div class="progress-container" style="
                                            background: #0a0a0a; 
                                            border-radius: 8px; 
                                            height: 12px; 
                                            overflow: hidden;
                                            border: 1px solid #333;
                                        ">
                                            <div class="progress-bar" style="
                                                background: linear-gradient(90deg, #00d4ff, #0099cc, #00d4ff); 
                                                height: 100%; 
                                                width: \${session.progress || 0}%; 
                                                transition: width 0.8s ease;
                                                box-shadow: 0 0 12px rgba(0, 212, 255, 0.6);
                                                animation: shimmer 1.5s infinite;
                                            "></div>
                                        </div>
                                        <div style="display: flex; justify-content: space-between; color: #888; font-size: 11px; margin-top: 6px;">
                                            <span>\${session.progress || 0}% Complete</span>
                                            <span>\${((session.graduationScore || 0) * 100).toFixed(1)}% Accuracy</span>
                                        </div>
                                    </div>
                                    
                                    \${session.logs && session.logs.length > 0 ? \`
                                        <div style="border-top: 1px solid #333; padding-top: 10px;">
                                            <div style="color: #666; font-size: 10px; margin-bottom: 4px;">Latest Activity:</div>
                                            <div style="color: #aaa; font-size: 11px;">
                                                \${session.logs.slice(-1)[0]}
                                            </div>
                                        </div>
                                    \` : ''}
                                </div>
                            \`).join('')}
                        </div>
                    \` : ''}
                    
                    <!-- Recently Completed -->
                    \${completed.length > 0 ? \`
                        <div style="margin-top: 20px;">
                            <h3 style="color: #7c4dff; margin-bottom: 15px;">📋 Recent Completions</h3>
                            \${completed.slice(-3).map(session => \`
                                <div style="
                                    background: linear-gradient(135deg, #1a1a1a 0%, #2a2a3e 100%); 
                                    border: 1px solid \${session.status === 'graduated' ? '#4CAF50' : '#f44336'}; 
                                    border-radius: 8px; 
                                    padding: 12px; 
                                    margin-bottom: 8px; 
                                    animation: slideInLeft 0.5s ease-out;
                                ">
                                    <div style="display: flex; justify-content: space-between; align-items: center;">
                                        <div style="color: #e0e0e0; font-weight: bold;">
                                            \${session.status === 'graduated' ? '🎓' : '❌'} \${session.personaName}
                                        </div>
                                        <div style="color: \${session.status === 'graduated' ? '#4CAF50' : '#f44336'}; font-size: 12px;">
                                            \${session.status === 'graduated' ? 'Graduated' : 'Failed'}
                                        </div>
                                    </div>
                                    <div style="color: #888; font-size: 11px; margin-top: 4px;">
                                        \${session.specialization.replace(/_/g, ' ')} • \${((session.graduationScore || 0) * 100).toFixed(1)}% accuracy
                                        \${session.fineTuneId ? ' • LoRA: ' + session.fineTuneId.substring(0, 15) + '...' : ''}
                                    </div>
                                </div>
                            \`).join('')}
                        </div>
                    \` : ''}
                </div>
                
                <style>
                @keyframes fadeIn {
                    from { opacity: 0; transform: translateY(10px); }
                    to { opacity: 1; transform: translateY(0); }
                }
                @keyframes slideIn {
                    from { opacity: 0; transform: translateX(-20px); }
                    to { opacity: 1; transform: translateX(0); }
                }
                @keyframes slideInLeft {
                    from { opacity: 0; transform: translateX(20px); }
                    to { opacity: 1; transform: translateX(0); }
                }
                @keyframes pulseGlow {
                    from { box-shadow: 0 0 20px rgba(0, 212, 255, 0.3); }
                    to { box-shadow: 0 0 30px rgba(0, 212, 255, 0.5); }
                }
                @keyframes shimmer {
                    0% { background-position: -100% 0; }
                    100% { background-position: 100% 0; }
                }
                </style>
            \`;
        }
        
        // OLD POLLING FUNCTION REMOVED - Now using interrupt-driven push updates
        function updateAcademyDisplay() {
            console.log('🎓 Frontend: updateAcademyDisplay() called but DISABLED - using push updates instead');
        }
        
        // Expandable details functions for rich Academy widgets
        function toggleTrainingDetails(detailId) {
            const details = document.getElementById(detailId);
            if (details) {
                const isVisible = details.style.display !== 'none';
                details.style.display = isVisible ? 'none' : 'block';
                if (!isVisible) {
                    details.style.animation = 'fadeIn 0.2s ease-in';
                }
            }
        }
        
        function toggleGraduateDetails(detailId) {
            const details = document.getElementById(detailId);
            if (details) {
                const isVisible = details.style.display !== 'none';
                details.style.display = isVisible ? 'none' : 'block';
                if (!isVisible) {
                    details.style.animation = 'fadeIn 0.2s ease-in';
                }
            }
        }
        
        function retryTraining(personaName) {
            console.log('🔄 Retrying training for:', personaName);
            showAdvancedTrainingDialog(personaName);
        }
        
        function showAdvancedTrainingDialog(basePersonaName, specialization = 'ai_recommended_enhancement') {
            const dialog = document.createElement('div');
            dialog.style.cssText = \`
                position: fixed;
                top: 50%;
                left: 50%;
                transform: translate(-50%, -50%);
                background: linear-gradient(135deg, #0a0a1a 0%, #1a1a2e 30%, #2a2a3e 100%);
                border: 2px solid #00d4ff;
                border-radius: 16px;
                padding: 0;
                z-index: 10000;
                box-shadow: 0 0 40px rgba(0, 212, 255, 0.6), inset 0 0 20px rgba(0, 212, 255, 0.1);
                min-width: 500px;
                max-width: 600px;
                color: white;
                backdrop-filter: blur(10px);
                animation: slideIn 0.3s ease-out;
            \`;
            
            const baseName = basePersonaName.split('-')[0];
            
            dialog.innerHTML = \`
                <style>
                @keyframes slideIn {
                    from { opacity: 0; transform: translate(-50%, -60%) scale(0.9); }
                    to { opacity: 1; transform: translate(-50%, -50%) scale(1); }
                }
                
                .custom-slider {
                    -webkit-appearance: none;
                    width: 100%;
                    height: 8px;
                    border-radius: 4px;
                    background: linear-gradient(to right, #333 0%, #666 50%, #00d4ff 100%);
                    outline: none;
                    margin: 8px 0;
                }
                
                .custom-slider::-webkit-slider-thumb {
                    -webkit-appearance: none;
                    appearance: none;
                    width: 20px;
                    height: 20px;
                    border-radius: 50%;
                    background: linear-gradient(135deg, #00d4ff 0%, #0099cc 100%);
                    cursor: pointer;
                    box-shadow: 0 0 10px rgba(0, 212, 255, 0.8);
                    transition: all 0.2s ease;
                }
                
                .custom-slider::-webkit-slider-thumb:hover {
                    transform: scale(1.2);
                    box-shadow: 0 0 15px rgba(0, 212, 255, 1);
                }
                
                .parameter-group {
                    background: rgba(0, 212, 255, 0.05);
                    border: 1px solid rgba(0, 212, 255, 0.2);
                    border-radius: 8px;
                    padding: 15px;
                    margin-bottom: 15px;
                }
                
                .slider-container {
                    position: relative;
                    margin: 10px 0;
                }
                
                .slider-value {
                    position: absolute;
                    right: 0;
                    top: -25px;
                    background: #00d4ff;
                    color: #000;
                    padding: 2px 8px;
                    border-radius: 12px;
                    font-size: 10px;
                    font-weight: bold;
                    animation: pulse 2s infinite;
                }
                
                @keyframes pulse {
                    0%, 100% { opacity: 1; }
                    50% { opacity: 0.7; }
                }
                
                .preset-chip {
                    display: inline-block;
                    background: rgba(0, 212, 255, 0.2);
                    border: 1px solid #00d4ff;
                    color: #00d4ff;
                    padding: 5px 12px;
                    margin: 3px;
                    border-radius: 15px;
                    font-size: 10px;
                    cursor: pointer;
                    transition: all 0.2s ease;
                }
                
                .preset-chip:hover {
                    background: #00d4ff;
                    color: #000;
                    transform: scale(1.05);
                }
                
                .enhanced-textarea {
                    width: 100%;
                    min-height: 120px;
                    padding: 12px;
                    background: rgba(0, 0, 0, 0.4);
                    color: white;
                    border: 2px solid rgba(0, 212, 255, 0.3);
                    border-radius: 8px;
                    resize: vertical;
                    font-size: 12px;
                    line-height: 1.4;
                    font-family: 'Courier New', monospace;
                    transition: border-color 0.3s ease;
                }
                
                .enhanced-textarea:focus {
                    border-color: #00d4ff;
                    box-shadow: 0 0 10px rgba(0, 212, 255, 0.3);
                    outline: none;
                }
                </style>
                
                <!-- Header -->
                <div style="background: linear-gradient(135deg, #00d4ff 0%, #0099cc 100%); padding: 20px; border-radius: 16px 16px 0 0; text-align: center;">
                    <h3 style="color: #000; margin: 0 0 5px 0; font-size: 18px; font-weight: bold;">⚡ ADVANCED TRAINING PROTOCOL</h3>
                    <div style="color: rgba(0, 0, 0, 0.7); font-size: 12px;">Enhanced Academy training for \${baseName}</div>
                </div>
                
                <!-- Content -->
                <div style="padding: 25px;">
                    <!-- Training Rounds Slider -->
                    <div class="parameter-group">
                        <label style="color: #00d4ff; font-size: 13px; font-weight: bold; display: block; margin-bottom: 8px;">🎯 Training Rounds</label>
                        <div class="slider-container">
                            <div class="slider-value" id="rounds-value">150</div>
                            <input type="range" id="training-rounds" min="50" max="2000" value="150" class="custom-slider">
                        </div>
                        <div style="display: flex; justify-content: space-between; color: #888; font-size: 10px; margin-top: 5px;">
                            <span>50 (Quick Test)</span>
                            <span>500 (Standard)</span>
                            <span>1000 (Intensive)</span>
                            <span>2000 (Maximum)</span>
                        </div>
                    </div>
                    
                    <!-- Learning Rate Slider -->
                    <div class="parameter-group">
                        <label style="color: #00d4ff; font-size: 13px; font-weight: bold; display: block; margin-bottom: 8px;">📈 Learning Rate</label>
                        <div class="slider-container">
                            <div class="slider-value" id="learning-rate-value">0.001</div>
                            <input type="range" id="learning-rate" min="0.0001" max="0.01" step="0.0001" value="0.001" class="custom-slider">
                        </div>
                        <div style="display: flex; justify-content: space-between; color: #888; font-size: 10px; margin-top: 5px;">
                            <span>0.0001 (Conservative)</span>
                            <span>0.001 (Balanced)</span>
                            <span>0.01 (Aggressive)</span>
                        </div>
                    </div>
                    
                    <!-- Training Intensity -->
                    <div class="parameter-group">
                        <label style="color: #00d4ff; font-size: 13px; font-weight: bold; display: block; margin-bottom: 8px;">⚡ Training Intensity</label>
                        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 10px;">
                            <select id="training-intensity" style="
                                padding: 10px; 
                                background: rgba(0, 0, 0, 0.6); 
                                color: white; 
                                border: 2px solid rgba(0, 212, 255, 0.3); 
                                border-radius: 6px;
                                font-size: 12px;
                            ">
                                <option value="normal">🔥 Normal (1x)</option>
                                <option value="high">⚡ High (5x)</option>
                                <option value="extreme" selected>🚀 Extreme (10x)</option>
                                <option value="gpu_low">🖥️ GPU Low (5x)</option>
                                <option value="gpu_medium">🖥️ GPU Med (10x)</option>
                                <option value="gpu_high">🖥️ GPU High (15x)</option>
                                <option value="gpu_max">🔥🖥️ GPU MAX (20x)</option>
                            </select>
                            
                            <div style="display: flex; align-items: center; gap: 8px;">
                                <input type="checkbox" id="enable-dropout" checked style="accent-color: #00d4ff;">
                                <label for="enable-dropout" style="color: #aaa; font-size: 11px;">Enable Dropout</label>
                            </div>
                        </div>
                    </div>
                    
                    <!-- Quick Presets -->
                    <div class="parameter-group">
                        <label style="color: #00d4ff; font-size: 13px; font-weight: bold; display: block; margin-bottom: 8px;">🎛️ Quick Presets</label>
                        <div>
                            <span class="preset-chip" onclick="applyPreset('quick')">⚡ Quick (100 rounds)</span>
                            <span class="preset-chip" onclick="applyPreset('balanced')">⚖️ Balanced (500 rounds)</span>
                            <span class="preset-chip" onclick="applyPreset('intensive')">🔥 Intensive (1000 rounds)</span>
                            <span class="preset-chip" onclick="applyPreset('maximum')">🚀 Maximum (2000 rounds)</span>
                        </div>
                    </div>
                    
                    <!-- Custom Training Focus -->
                    <div class="parameter-group">
                        <label style="color: #00d4ff; font-size: 13px; font-weight: bold; display: block; margin-bottom: 8px;">📝 Training Objectives</label>
                        <div style="display: flex; gap: 5px; margin-bottom: 8px; flex-wrap: wrap;">
                            <span class="preset-chip" onclick="addPromptText('Improve accuracy')">🎯 Accuracy</span>
                            <span class="preset-chip" onclick="addPromptText('Enhanced protocol compliance')">🛡️ Protocol</span>
                            <span class="preset-chip" onclick="addPromptText('Leadership coordination')">👥 Leadership</span>
                            <span class="preset-chip" onclick="addPromptText('Error reduction')">🔧 Error Fix</span>
                        </div>
                        <textarea id="custom-prompt" class="enhanced-textarea" placeholder="Enter detailed training objectives...

Example:
• Focus on accuracy improvements above 90%
• Enhance decision-making under pressure
• Improve multi-agent coordination
• Strengthen protocol compliance">\${basePersonaName.includes('PlannerAI') ? 'Enhanced leadership training for AI coordination and command execution.\\n\\nObjectives:\\n• Improve accuracy above 90%\\n• Enhanced protocol compliance\\n• Multi-agent orchestration capabilities\\n• Robust error handling and recovery' : 'Advanced Academy training with enhanced performance targets.\\n\\nObjectives:\\n• Achieve 85%+ accuracy consistently\\n• Improve response time and efficiency\\n• Enhanced decision-making capabilities\\n• Robust performance under stress'}</textarea>
                    </div>
                    
                    <!-- Training Estimate -->
                    <div style="
                        background: linear-gradient(135deg, rgba(0, 212, 255, 0.1) 0%, rgba(0, 153, 204, 0.1) 100%); 
                        border: 1px solid rgba(0, 212, 255, 0.3); 
                        border-radius: 10px; 
                        padding: 15px; 
                        margin-bottom: 20px;
                    ">
                        <div style="color: #00d4ff; font-size: 13px; font-weight: bold; margin-bottom: 8px;">📊 Training Forecast</div>
                        <div id="training-estimate" style="color: #ccc; font-size: 12px; line-height: 1.4;">
                            • Duration: ~5 minutes<br>
                            • Effective Rounds: 1,500<br>
                            • Accuracy Target: 85%+<br>
                            • Enhanced Algorithm: Active
                        </div>
                    </div>
                    
                    <!-- Action Buttons -->
                    <div style="display: flex; gap: 12px; justify-content: center;">
                        <button onclick="startAdvancedTraining()" style="
                            background: linear-gradient(135deg, #4CAF50 0%, #45a049 100%); 
                            color: white; 
                            border: none; 
                            padding: 12px 25px; 
                            border-radius: 8px; 
                            cursor: pointer;
                            font-weight: bold;
                            font-size: 13px;
                            box-shadow: 0 0 15px rgba(76, 175, 80, 0.4);
                            transition: all 0.2s ease;
                        " onmouseover="this.style.transform='scale(1.05)'" onmouseout="this.style.transform='scale(1)'">
                            🚀 Launch Advanced Training
                        </button>
                        <button onclick="closeAdvancedDialog()" style="
                            background: linear-gradient(135deg, #666 0%, #555 100%); 
                            color: white; 
                            border: none; 
                            padding: 12px 25px; 
                            border-radius: 8px; 
                            cursor: pointer;
                            font-size: 13px;
                            transition: all 0.2s ease;
                        " onmouseover="this.style.transform='scale(1.05)'" onmouseout="this.style.transform='scale(1)'">
                            Cancel
                        </button>
                    </div>
                </div>
            \`;
            
            document.body.appendChild(dialog);
            window.currentTrainingDialog = dialog;
            window.currentBasePersonaName = baseName;
            window.currentSpecialization = specialization;
            
            // Get all control elements
            const roundsSlider = dialog.querySelector('#training-rounds');
            const learningRateSlider = dialog.querySelector('#learning-rate');
            const intensitySelect = dialog.querySelector('#training-intensity');
            const estimateDiv = dialog.querySelector('#training-estimate');
            const roundsValue = dialog.querySelector('#rounds-value');
            const learningRateValue = dialog.querySelector('#learning-rate-value');
            
            // Global functions for preset actions
            window.applyPreset = function(preset) {
                const presets = {
                    quick: { rounds: 100, intensity: 'high', learningRate: 0.005 },
                    balanced: { rounds: 500, intensity: 'extreme', learningRate: 0.001 },
                    intensive: { rounds: 1000, intensity: 'gpu_medium', learningRate: 0.0005 },
                    maximum: { rounds: 2000, intensity: 'gpu_max', learningRate: 0.0003 }
                };
                
                const config = presets[preset];
                if (config) {
                    roundsSlider.value = config.rounds;
                    intensitySelect.value = config.intensity;
                    learningRateSlider.value = config.learningRate;
                    updateEstimate();
                }
            };
            
            window.addPromptText = function(text) {
                const textarea = dialog.querySelector('#custom-prompt');
                if (textarea.value.trim()) {
                    textarea.value += '\\n• ' + text;
                } else {
                    textarea.value = '• ' + text;
                }
                textarea.focus();
            };
            
            function updateEstimate() {
                const rounds = parseInt(roundsSlider.value);
                const learningRate = parseFloat(learningRateSlider.value);
                const intensity = intensitySelect.value;
                
                // Update display values
                roundsValue.textContent = rounds;
                learningRateValue.textContent = learningRate.toFixed(4);
                
                const multipliers = {
                    normal: 1, high: 5, extreme: 10, 
                    gpu_low: 5, gpu_medium: 10, gpu_high: 15, gpu_max: 20
                };
                const mult = multipliers[intensity] || 1;
                const effectiveRounds = rounds * mult;
                const estimatedMinutes = Math.round(effectiveRounds / 100);
                const accuracyTarget = Math.min(98, 75 + (effectiveRounds / 200));
                
                estimateDiv.innerHTML = \`
                    • Duration: ~\${estimatedMinutes} minutes<br>
                    • Effective Rounds: \${effectiveRounds.toLocaleString()}<br>
                    • Learning Rate: \${learningRate.toFixed(4)}<br>
                    • Accuracy Target: \${accuracyTarget.toFixed(1)}%+<br>
                    • Enhanced Algorithm: Active
                \`;
            }
            
            // Event listeners for all controls
            roundsSlider.addEventListener('input', updateEstimate);
            learningRateSlider.addEventListener('input', updateEstimate);
            intensitySelect.addEventListener('change', updateEstimate);
            updateEstimate();
        }
        
        function startAdvancedTraining() {
            const dialog = window.currentTrainingDialog;
            const rounds = parseInt(dialog.querySelector('#training-rounds').value);
            const learningRate = parseFloat(dialog.querySelector('#learning-rate').value);
            const intensity = dialog.querySelector('#training-intensity').value;
            const customPrompt = dialog.querySelector('#custom-prompt').value;
            const enableDropout = dialog.querySelector('#enable-dropout').checked;
            const baseName = window.currentBasePersonaName;
            const specialization = window.currentSpecialization || 'enhanced_training';
            
            const newPersonaName = baseName + '-enhanced-' + Date.now();
            
            console.log('🚀 Starting advanced training with parameters:', {
                rounds, learningRate, intensity, enableDropout
            });
            
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'start_academy_training',
                    personaName: newPersonaName,
                    specialization: specialization,
                    rounds: rounds,
                    learningRate: learningRate,
                    enableDropout: enableDropout,
                    customPrompt: customPrompt,
                    trainingIntensity: intensity
                }));
                addSystemMessage(\`🚀 \${newPersonaName} starting enhanced training: \${rounds} rounds at \${intensity} intensity...\`);
            }
            
            closeAdvancedDialog();
        }
        
        function closeAdvancedDialog() {
            if (window.currentTrainingDialog) {
                document.body.removeChild(window.currentTrainingDialog);
                window.currentTrainingDialog = null;
                window.currentBasePersonaName = null;
                window.currentSpecialization = null;
            }
        }
        
        function toggleAcademyDetails(personaName) {
            console.log('Toggle details for:', personaName);
        }
        
        function showPendingTrainingButton() {
            const button = document.getElementById('pending-training-btn');
            if (button && window.pendingRetrain) {
                button.style.display = 'inline-flex';
                button.textContent = \`🚀 Start \${window.pendingRetrain.personaName} Training\`;
            }
        }
        
        function hidePendingTrainingButton() {
            const button = document.getElementById('pending-training-btn');
            if (button) {
                button.style.display = 'none';
            }
        }
        
        function startPendingTraining() {
            if (!window.pendingRetrain) {
                addSystemMessage('❌ No pending training found');
                return;
            }
            
            const { personaName, improvements } = window.pendingRetrain;
            
            // Start training immediately with the pending data
            const specialization = 'enhanced_capabilities';
            const description = \`Custom enhancement training: \${improvements}\`;
            
            addSystemMessage(\`🎓 Starting Academy training for \${personaName} with custom enhancements...\`);
            
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'start_academy_training',
                    personaName: personaName + '-enhanced-' + Date.now(),
                    specialization: specialization,
                    rounds: 15,
                    customPrompt: \`Training Focus: \${improvements}\\n\\nSpecialization: enhanced_capabilities\\n\\nDescription: \${description}\`,
                    trainingIntensity: 'high'
                }));
                
                // Clear pending training
                window.pendingRetrain = null;
                hidePendingTrainingButton();
                
                addMessage('Academy System', \`🎓 \${personaName} sent to Academy for enhanced capabilities training!\`, 'ai', true);
            }
        }
        
        // Auto-resize textarea
        document.getElementById('messageInput').addEventListener('input', function() {
            autoResize(this);
        });
        
        // Initialize
        initWebSocket();
        
        // Initialize default room
        document.addEventListener('DOMContentLoaded', function() {
            // Load chat history before initializing
            const historyLoaded = loadChatHistory();
            
            // Initialize with current room (from history or default)
            switchRoom(currentRoom);
            
            // Select the correct agent from history
            if (selectedAgent !== 'auto') {
                selectAgent(selectedAgent);
            }
            
            refreshProjects(); // Load active projects
            refreshPersonas(); // Load personas on startup
            
            if (historyLoaded && Object.values(roomMessages).flat().length > 0) {
                // Only show restore message if there was actual content restored
                const totalMessages = Object.values(roomMessages).flat().length;
                if (totalMessages > 0) {
                    // Use skipSave=true so this message doesn't get saved to localStorage
                    addMessage('System', \`📚 Restored \${totalMessages} messages from previous session\`, 'ai', false, true);
                }
            }
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

    <!-- External Libraries -->
    <script src="https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js"></script>
    
    <!-- Component System -->
    <script src="/src/ui/utils/ComponentLoader.js"></script>
    <script src="/src/ui/components/AgentSelector.js"></script>
    <script>
        // Initialize component system
        document.addEventListener('DOMContentLoaded', () => {
            console.log('🔧 Component system initialized');
            
            // For now, keep existing agent selector but prepare for migration
            // Future: Replace .agent-selector with <agent-selector> component
            
            window.componentSystemReady = true;
            document.dispatchEvent(new CustomEvent('component-system-ready'));
            
            // Reload loop protection
            window.reloadAttempts = 0;
            window.maxReloads = 3;
            window.reloadTracker = {
                count: 0,
                lastReload: 0,
                checkReloadLoop: function() {
                    const now = Date.now();
                    if (now - this.lastReload < 5000) { // Less than 5 seconds since last reload
                        this.count++;
                        if (this.count >= window.maxReloads) {
                            console.error('🚨 RELOAD LOOP DETECTED - BLOCKING FURTHER RELOADS');
                            addSystemMessage('🚨 Reload loop detected and blocked for safety');
                            return false;
                        }
                    } else {
                        this.count = 0; // Reset if enough time has passed
                    }
                    this.lastReload = now;
                    return true;
                }
            };
        });
    </script>
</body>
</html>`;

    // Apply progressive web components enhancement
    return this.webComponentsIntegration.enhanceHTML(html);
  }

  generateConversationHistory() {
    return `
        // Load any existing conversation history here
        // This could come from a database or session storage
    `;
  }
}

module.exports = UIGenerator;