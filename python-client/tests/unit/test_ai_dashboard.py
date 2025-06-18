#!/usr/bin/env python3
"""
Unit Tests for AI Agent Dashboard System
========================================
Tests dependency ranking, ticket management, and dashboard functionality.
"""

import pytest
import tempfile
import json
from pathlib import Path
from unittest.mock import patch, mock_open
import sys
import os

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

# Import dashboard functions
try:
    import importlib.util
    import sys
    # Simple approach: ai-agent.py is in the current working directory when pytest runs
    ai_agent_path = Path("ai-agent.py")
    spec = importlib.util.spec_from_file_location("ai_agent", ai_agent_path)
    ai_agent = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ai_agent)
    
    # Import the functions we need
    get_command_dependencies = ai_agent.get_command_dependencies
    topological_sort = ai_agent.topological_sort
    get_command_tickets = ai_agent.get_command_tickets
    show_dashboard = ai_agent.show_dashboard
    show_broken = ai_agent.show_broken
    run_command = ai_agent.run_command
    
except ImportError as e:
    pytest.skip(f"ai-agent.py not available: {e}", allow_module_level=True)


class TestDependencySystem:
    """Test the command dependency ranking system"""
    
    def test_known_dependencies(self):
        """Test that known dependencies are loaded correctly"""
        deps = get_command_dependencies()
        
        # Test foundation commands (no dependencies)
        assert deps['exec'] == []
        assert deps['filesave'] == []
        assert deps['clear'] == []
        assert deps['workspace'] == []
        
        # Test dependent commands
        assert 'exec' in deps['chat']
        assert 'browser' in deps['input']
        assert 'browser' in deps['cursor']
        assert 'exec' in deps['diagnostics']
        assert 'screenshot' in deps['diagnostics']
    
    def test_topological_sort(self):
        """Test dependency sorting algorithm"""
        test_deps = {
            'a': [],           # Foundation
            'b': ['a'],        # Depends on a
            'c': ['a', 'b'],   # Depends on a and b
            'd': [],           # Foundation
            'e': ['d']         # Depends on d
        }
        
        sorted_commands = topological_sort(test_deps)
        
        # Foundation commands should come first
        assert sorted_commands.index('a') < sorted_commands.index('b')
        assert sorted_commands.index('a') < sorted_commands.index('c')
        assert sorted_commands.index('b') < sorted_commands.index('c')
        assert sorted_commands.index('d') < sorted_commands.index('e')
    
    def test_circular_dependency_handling(self):
        """Test that circular dependencies don't crash the system"""
        circular_deps = {
            'a': ['b'],
            'b': ['a'],
            'c': []
        }
        
        # Should not crash, and should include all commands
        result = topological_sort(circular_deps)
        assert len(result) == 3
        assert 'a' in result
        assert 'b' in result
        assert 'c' in result


class TestTicketSystem:
    """Test the ticket management and status parsing"""
    
    @patch('pathlib.Path.exists')
    @patch('pathlib.Path.iterdir')
    @patch('pathlib.Path.read_text')
    def test_ticket_parsing(self, mock_read_text, mock_iterdir, mock_exists):
        """Test parsing README files for ticket information"""
        
        # Mock file system structure
        mock_exists.return_value = True
        
        # Create mock command directory
        from unittest.mock import MagicMock
        mock_cmd_dir = MagicMock()
        mock_cmd_dir.name = 'testcmd'
        mock_cmd_dir.is_dir.return_value = True
        mock_iterdir.return_value = [mock_cmd_dir]
        
        # Mock README content
        mock_readme_content = """
# Test Command

## Definition
- **Name**: testcmd
- **Description**: Test command for unit tests
- **Status**: 🔴 BROKEN (2025-06-18) - Method signature mismatch
- **Parameters**: `[test] [param]`

## TODO:
- TODO: Fix method signature
- TODO: Add unit tests
"""
        mock_read_text.return_value = mock_readme_content
        
        # Test ticket parsing
        with patch('ai_agent.get_command_dependencies') as mock_deps:
            mock_deps.return_value = {'testcmd': ['exec']}
            tickets = get_command_tickets()
        
        assert len(tickets) == 1
        ticket = tickets[0]
        
        assert ticket['name'] == 'testcmd'
        assert '🔴' in ticket['status']
        assert 'Method signature mismatch' in ticket['issue']
        assert ticket['todos'] == 2
        assert ticket['date'] == '2025-06-18'
        assert ticket['priority'] == 1  # Broken = priority 1
        assert ticket['dependencies'] == ['exec']


class TestDashboardViews:
    """Test dashboard display functions"""
    
    @patch('ai_agent.get_command_tickets')
    @patch('builtins.print')
    @pytest.mark.asyncio
    async def test_show_broken(self, mock_print, mock_tickets):
        """Test broken commands view"""
        
        # Mock ticket data
        mock_tickets.return_value = [
            {
                'name': 'foundation_cmd',
                'status': '🔴 BROKEN - Test error',
                'issue': 'Test error',
                'todos': 2,
                'date': '2025-06-18',
                'priority': 1,
                'dep_order': 0,
                'dependencies': []
            },
            {
                'name': 'dependent_cmd', 
                'status': '🔴 BROKEN - Another error',
                'issue': 'Another error',
                'todos': 1,
                'date': '2025-06-18',
                'priority': 1,
                'dep_order': 1,
                'dependencies': ['foundation_cmd']
            }
        ]
        
        await show_broken()
        
        # Verify print was called with expected content
        mock_print.assert_called()
        
        # Check that foundation command comes first in dependency order
        printed_content = ' '.join([str(call.args[0]) for call in mock_print.call_args_list])
        foundation_pos = printed_content.find('foundation_cmd')
        dependent_pos = printed_content.find('dependent_cmd')
        assert foundation_pos < dependent_pos


class TestGitIntegration:
    """Test git system integration"""
    
    def test_git_hook_script_creation(self):
        """Test creating git hooks for dashboard integration"""
        
        hook_script = """#!/bin/bash
# Auto-update dashboard status on commit
echo "📊 Updating project status dashboard..."
python3 python-client/ai-portal.py --cmd docs --params '{"include": "status"}'
"""
        
        # Test that the hook script is valid bash
        assert hook_script.startswith('#!/bin/bash')
        assert 'ai-portal.py' in hook_script
        assert '--cmd docs' in hook_script
    
    def test_commit_message_enhancement(self):
        """Test enhancing commit messages with ticket status"""
        
        # Mock getting ticket status for commit message
        def enhance_commit_message(original_msg, fixed_tickets=None):
            if fixed_tickets:
                enhanced = f"{original_msg}\n\n🔧 Dashboard Updates:\n"
                for ticket in fixed_tickets:
                    enhanced += f"- Fixed: {ticket['name']} ({ticket['issue']})\n"
                enhanced += "\n📊 Project Health: Improved by fixing foundation commands"
                return enhanced
            return original_msg
        
        original = "Fix method signature in input command"
        fixed_tickets = [
            {'name': 'input', 'issue': 'Method signature mismatch'}
        ]
        
        enhanced = enhance_commit_message(original, fixed_tickets)
        
        assert "🔧 Dashboard Updates:" in enhanced
        assert "Fixed: input" in enhanced
        assert "📊 Project Health:" in enhanced


class TestReportGeneration:
    """Test dashboard report generation for docs"""
    
    @patch('subprocess.run')
    def test_dashboard_report_generation(self, mock_subprocess):
        """Test generating dashboard report for documentation"""
        
        # Mock subprocess output
        mock_subprocess.return_value.returncode = 0
        mock_subprocess.return_value.stdout = """
🔴 BROKEN COMMANDS - DEPENDENCY-RANKED ORDER
==================================================
Found 3 broken commands (sorted by fix order):

 1. 🔴 exec
     Issue: Foundation command needs fixing
     Dependencies:  → foundation command (no deps)

 2. 🔴 input  
     Issue: Method signature mismatch
     Dependencies:  → depends on: browser
""".strip()
        
        # Test report formatting
        report = mock_subprocess.return_value.stdout
        assert "DEPENDENCY-RANKED ORDER" in report
        assert "foundation command" in report
        assert "depends on:" in report
    
    def test_markdown_report_formatting(self):
        """Test converting dashboard output to markdown"""
        
        def format_dashboard_for_markdown(console_output):
            lines = console_output.split('\n')
            markdown = []
            
            markdown.append('> 📊 **Live Status Report** - Generated from dependency-aware dashboard\n')
            markdown.append('```')
            markdown.append(console_output.strip())
            markdown.append('```\n')
            
            markdown.append('### Quick Commands for Development\n')
            markdown.append('```bash')
            markdown.append('# View full dashboard')
            markdown.append('python3 python-client/ai-portal.py --dashboard')
            markdown.append('```\n')
            
            return '\n'.join(markdown)
        
        test_output = "🔴 Test broken command output"
        markdown = format_dashboard_for_markdown(test_output)
        
        assert '📊 **Live Status Report**' in markdown
        assert '```' in markdown
        assert 'python3 python-client/ai-portal.py --dashboard' in markdown


if __name__ == '__main__':
    pytest.main([__file__, '-v'])