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


def get_continuum_ws_url() -> str:
    """Get Continuum WebSocket URL from environment"""
    load_continuum_config()
    return os.getenv('CONTINUUM_WS_URL', 'ws://localhost:9000')


def get_continuum_ws_port() -> int:
    """Get Continuum WebSocket port from environment"""
    load_continuum_config()
    return int(os.getenv('CONTINUUM_WS_PORT', '9000'))


def get_continuum_port() -> int:
    """Get Continuum HTTP port from environment"""
    load_continuum_config()
    return int(os.getenv('CONTINUUM_PORT', '9000'))