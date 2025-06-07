"""
Continuum Configuration Utilities
Loads configuration from ~/.continuum/config.env
"""

import os
from pathlib import Path
from typing import Optional


def load_continuum_config() -> None:
    """Load Continuum configuration from ~/.continuum/config.env"""
    config_path = Path.home() / '.continuum' / 'config.env'
    if config_path.exists():
        with open(config_path) as f:
            for line in f:
                if '=' in line and not line.strip().startswith('#'):
                    key, value = line.strip().split('=', 1)
                    os.environ[key] = value


def get_continuum_port() -> int:
    """Get Continuum port from environment"""
    load_continuum_config()
    return int(os.getenv('CONTINUUM_PORT', '9000'))


def get_continuum_ws_port() -> int:
    """Get Continuum WebSocket port (same as HTTP port)"""
    return get_continuum_port()


def get_continuum_ws_url() -> str:
    """Get Continuum WebSocket URL (calculated from port)"""
    port = get_continuum_port()
    return f'ws://localhost:{port}'