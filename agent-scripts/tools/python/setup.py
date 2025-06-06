#!/usr/bin/env python3
"""
Setup script for agent-scripts with venv in .continuum/venv
"""

import os
import sys
import subprocess
import venv
from pathlib import Path

def main():
    # Find .continuum directory
    continuum_dir = Path(__file__).parent.parent / ".continuum"
    continuum_dir.mkdir(exist_ok=True)
    
    # Create venv subdirectory for different environments
    venv_base_dir = continuum_dir / "venv"
    venv_base_dir.mkdir(exist_ok=True)
    
    venv_dir = venv_base_dir / "agents"
    
    print(f"🛰️ Setting up agent-scripts environment...")
    print(f"📁 Virtual environment: {venv_dir}")
    
    # Create venv if it doesn't exist
    if not venv_dir.exists():
        print("🔨 Creating virtual environment...")
        venv.create(venv_dir, with_pip=True)
        print("✅ Virtual environment created")
    else:
        print("✅ Virtual environment already exists")
    
    # Install requirements
    requirements_file = Path(__file__).parent / "requirements.txt"
    if requirements_file.exists():
        print("📦 Installing requirements...")
        pip_path = venv_dir / "bin" / "pip"
        if not pip_path.exists():
            pip_path = venv_dir / "Scripts" / "pip.exe"  # Windows
        
        result = subprocess.run([
            str(pip_path), "install", "-r", str(requirements_file)
        ], capture_output=True, text=True)
        
        if result.returncode == 0:
            print("✅ Requirements installed")
        else:
            print(f"❌ Failed to install requirements: {result.stderr}")
            sys.exit(1)
    
    # Create activation script
    activation_script = Path(__file__).parent / "activate-env.sh"
    python_path = venv_dir / "bin" / "python"
    if not python_path.exists():
        python_path = venv_dir / "Scripts" / "python.exe"  # Windows
    
    activation_script.write_text(f"""#!/bin/bash
# Activate agent-scripts environment
export AGENT_PYTHON="{python_path}"
echo "🛰️ Agent environment activated"
echo "Python: $AGENT_PYTHON"
""")
    activation_script.chmod(0o755)
    
    # Update scripts to use the venv python
    scripts_to_update = ["js-send.py", "probe-safe.py"]
    for script_name in scripts_to_update:
        script_path = Path(__file__).parent / script_name
        if script_path.exists():
            content = script_path.read_text()
            # Update shebang to use venv python
            if content.startswith("#!/usr/bin/env python3"):
                content = f"#!{python_path}\n" + content[len("#!/usr/bin/env python3\n"):]
                script_path.write_text(content)
                print(f"✅ Updated {script_name} to use venv python")
    
    print("\n🟢 Setup complete!")
    print(f"📁 Virtual environment: {venv_dir}")
    print(f"🐍 Python: {python_path}")
    print("\n📋 Usage:")
    print("  source agent-scripts/activate-env.sh")
    print("  ./agent-scripts/probe-safe.py status")

if __name__ == "__main__":
    main()