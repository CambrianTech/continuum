"""
Command Discovery Module
Handles dynamic command discovery and method creation
"""

import aiohttp
from typing import Dict, Any, Callable, Optional
from .types import ContinuumCommand, CommandResult

class CommandDiscovery:
    """
    Discovers and caches commands from running Continuum system
    Creates dynamic Python methods for discovered commands
    """
    
    def __init__(self, session: aiohttp.ClientSession, api_url: str):
        self.session = session
        self.api_url = api_url
        self._commands_cache: Optional[Dict[str, ContinuumCommand]] = None
    
    async def discover_commands(self) -> Dict[str, ContinuumCommand]:
        """Discover all available commands from system"""
        if self._commands_cache:
            return self._commands_cache
        
        try:
            async with self.session.get(f"{self.api_url}/commands") as response:
                if response.status == 200:
                    data = await response.json()
                    commands = {}
                    
                    for name, info in data.get('commands', {}).items():
                        commands[name] = ContinuumCommand(
                            name=name,
                            description=info.get('description', ''),
                            parameters=info.get('parameters', {}),
                            category=info.get('category', 'general'),
                            examples=info.get('examples', [])
                        )
                    
                    self._commands_cache = commands
                    return commands
        except Exception as e:
            print(f"⚠️ Command discovery failed: {e}")
        
        return {}
    
    def get_command_method(self, method_name: str) -> Callable:
        """
        Create async method for discovered command
        Converts snake_case to kebab-case automatically
        """
        command_name = method_name.replace('_', '-')
        
        async def command_executor(*args, **kwargs) -> CommandResult:
            # Ensure commands are discovered
            await self.discover_commands()
            
            # Build request payload
            payload = {
                'command': command_name,
                'args': kwargs
            }
            
            if args:
                payload['positional_args'] = list(args)
            
            try:
                async with self.session.post(
                    f"{self.api_url}/command",
                    json=payload,
                    timeout=30
                ) as response:
                    
                    if response.status == 200:
                        result = await response.json()
                        if result.get('success'):
                            return result.get('data')
                        else:
                            raise RuntimeError(f"Command '{command_name}' failed: {result.get('error')}")
                    else:
                        text = await response.text()
                        raise RuntimeError(f"API error {response.status}: {text}")
                        
            except aiohttp.ClientError as e:
                raise ConnectionError(f"Failed to execute '{command_name}': {e}")
        
        # Add metadata from discovered command
        if self._commands_cache and command_name in self._commands_cache:
            cmd = self._commands_cache[command_name]
            command_executor.__doc__ = f"{cmd.description}\n\nCategory: {cmd.category}"
            command_executor.__annotations__ = self._build_annotations(cmd)
        
        return command_executor
    
    def _build_annotations(self, command: ContinuumCommand) -> Dict[str, type]:
        """Build type annotations from command parameters"""
        annotations = {}
        
        for param_name, param_info in command.parameters.items():
            param_type = param_info.get('type', 'any')
            
            # Map JSON schema types to Python types
            type_mapping = {
                'string': str,
                'number': float,
                'integer': int,
                'boolean': bool,
                'array': list,
                'object': dict
            }
            
            annotations[param_name] = type_mapping.get(param_type, Any)
        
        annotations['return'] = CommandResult
        return annotations
    
    async def refresh_commands(self) -> Dict[str, ContinuumCommand]:
        """Force refresh of command cache"""
        self._commands_cache = None
        return await self.discover_commands()
    
    def get_command_info(self, command_name: str) -> Optional[ContinuumCommand]:
        """Get detailed info about a specific command"""
        if self._commands_cache:
            return self._commands_cache.get(command_name.replace('_', '-'))
        return None