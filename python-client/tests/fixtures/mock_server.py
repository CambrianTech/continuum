"""
Mock WebSocket server for testing
"""
import time
from typing import Dict, Any
from continuum_client.exceptions.js_errors import ConnectionError

class MockWebSocketServer:
    """Mock WebSocket server that simulates Continuum server behavior"""
    
    def __init__(self):
        self.connected_agents = {}
        self.message_history = []
        self.crash_simulation = False
        self.restart_count = 0
        
    def simulate_crash(self):
        """Simulate server crash for recovery testing"""
        self.crash_simulation = True
        
    def simulate_restart(self):
        """Simulate server restart"""
        self.restart_count += 1
        self.crash_simulation = False
        
    def register_agent(self, agent_info: Dict[str, Any]) -> Dict[str, Any]:
        """Simulate agent registration"""
        if self.crash_simulation:
            raise ConnectionError("Server crashed during registration")
            
        self.connected_agents[agent_info['agentId']] = agent_info
        return {
            'type': 'agent_registered',
            'success': True,
            'agentId': agent_info['agentId'],
            'timestamp': time.time()
        }
        
    def execute_js(self, js_code: str, execution_id: str) -> Dict[str, Any]:
        """Simulate JavaScript execution with various scenarios"""
        if self.crash_simulation:
            raise ConnectionError("Server crashed during JS execution")
            
        # Error scenarios
        if 'undefined_variable' in js_code:
            return {
                'type': 'js_executed',
                'success': False,
                'result': None,
                'error': 'ReferenceError: undefined_variable is not defined',
                'executionId': execution_id
            }
        
        # DOM query scenarios
        if 'document.querySelector' in js_code and 'agent-item' in js_code:
            return {
                'type': 'js_executed',
                'success': True,
                'result': [
                    {'tagName': 'DIV', 'className': 'agent-item', 'textContent': 'Fred', 'dataset': {'agentId': 'fred'}},
                    {'tagName': 'DIV', 'className': 'agent-item', 'textContent': 'Joel', 'dataset': {'agentId': 'user-joel'}}
                ],
                'output': [{'level': 'log', 'message': 'DOM queried successfully'}],
                'executionId': execution_id
            }
        
        # HTML content scenarios
        if 'innerHTML' in js_code:
            mock_html = '''
            <div class="agent-selector">
                <div class="agent-item" data-agent-id="fred">Fred</div>
                <div class="agent-item" data-agent-id="user-joel">Joel</div>
            </div>
            '''
            return {
                'type': 'js_executed',
                'success': True,
                'result': mock_html.strip(),
                'output': [],
                'executionId': execution_id
            }
        
        # Default success
        return {
            'type': 'js_executed',
            'success': True,
            'result': 'Success',
            'output': [{'level': 'log', 'message': f'Executed: {js_code[:50]}...'}],
            'executionId': execution_id
        }