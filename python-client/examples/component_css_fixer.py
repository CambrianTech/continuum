#!/usr/bin/env python3
"""
Component CSS Fixer - Quick tool for applying CSS fixes to web components
🔧 Apply CSS fixes to shadow DOM components or regular DOM elements
"""
import asyncio
import json
import sys

sys.path.append('/Users/joel/Development/ideem/vHSM/externals/continuum/python-client')

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class ComponentCSSFixer:
    """Quick utility to apply CSS fixes to components"""
    
    @staticmethod
    async def apply_css_to_component(client, selector, css_rules, description="CSS fixes"):
        """
        Apply CSS rules to a component
        
        Args:
            client: ContinuumClient instance
            selector: CSS selector for target component
            css_rules: CSS string to apply
            description: Description of what's being fixed
        
        Returns:
            bool: Success status
        """
        print(f"🔧 Applying {description} to '{selector}'...")
        
        fix_js = f"""
        return new Promise((resolve) => {{
            const element = document.querySelector('{selector}');
            if (!element) {{
                resolve(JSON.stringify({{
                    success: false,
                    error: 'Element not found: {selector}'
                }}));
                return;
            }}
            
            console.log('Found element:', element);
            
            // Handle shadow DOM components
            if (element.shadowRoot) {{
                console.log('Applying to shadow DOM...');
                let styleElement = element.shadowRoot.querySelector('style');
                
                if (!styleElement) {{
                    // Create style element if it doesn't exist
                    styleElement = document.createElement('style');
                    element.shadowRoot.appendChild(styleElement);
                }}
                
                // Append new CSS rules
                styleElement.textContent += `\\n{css_rules}`;
                
                resolve(JSON.stringify({{
                    success: true,
                    message: 'CSS applied to shadow DOM component',
                    target: 'shadow-dom'
                }}));
                return;
            }}
            
            // Handle regular DOM elements
            console.log('Applying to regular DOM...');
            const globalStyleId = 'component-css-fixes-' + '{selector}'.replace(/[^a-zA-Z0-9]/g, '-');
            let globalStyleElement = document.getElementById(globalStyleId);
            
            if (!globalStyleElement) {{
                globalStyleElement = document.createElement('style');
                globalStyleElement.id = globalStyleId;
                document.head.appendChild(globalStyleElement);
            }}
            
            // Apply CSS rules scoped to the selector
            const scopedCSS = `{selector} {{ {css_rules} }}`;
            globalStyleElement.textContent += '\\n' + scopedCSS;
            
            resolve(JSON.stringify({{
                success: true,
                message: 'CSS applied to regular DOM element',
                target: 'regular-dom'
            }}));
        }});
        """
        
        try:
            result = await client.js.get_value(fix_js, timeout=15)
            data = json.loads(result)
            
            if data.get('success'):
                print(f"✅ {description} applied successfully!")
                print(f"   Target: {data.get('target', 'unknown')}")
                return True
            else:
                print(f"❌ Failed to apply {description}: {data.get('error')}")
                return False
                
        except Exception as e:
            print(f"❌ Exception applying {description}: {e}")
            return False
    
    @staticmethod
    async def quick_style_fix(selector, css_rules, description="Quick styling fix"):
        """
        Standalone function to quickly apply CSS fixes
        
        Usage:
            await ComponentCSSFixer.quick_style_fix(
                'agent-selector',
                '.title { font-weight: 600; }',
                'Fix title weight'
            )
        """
        load_continuum_config()
        
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'css-fixer',
                'agentName': 'CSS Fixer',
                'agentType': 'ai'
            })
            
            return await ComponentCSSFixer.apply_css_to_component(
                client, selector, css_rules, description
            )

# Pre-defined fix collections for common issues
class CommonCSSFixes:
    """Collection of common CSS fixes for Continuum components"""
    
    SEARCH_BOX_FIXES = """
        .search-container {
            position: relative;
            margin-bottom: 15px;
        }
        
        .search-input {
            width: 100%;
            padding: 8px 12px 8px 35px;
            background: rgba(255, 255, 255, 0.08);
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 8px;
            color: #e0e6ed;
            font-size: 13px;
            box-sizing: border-box;
            transition: all 0.3s ease;
        }
        
        .search-input::placeholder {
            color: #8a92a5;
        }
        
        .search-input:focus {
            outline: none;
            border-color: rgba(0, 212, 255, 0.5);
            background: rgba(255, 255, 255, 0.12);
            box-shadow: 0 0 8px rgba(0, 212, 255, 0.2);
        }
        
        .search-icon {
            position: absolute;
            left: 12px;
            top: 50%;
            transform: translateY(-50%);
            color: #8a92a5;
            font-size: 14px;
            pointer-events: none;
        }
    """
    
    TITLE_FIXES = """
        .title {
            font-size: 12px !important;
            color: #8a92a5 !important;
            margin-bottom: 15px !important;
            text-transform: uppercase !important;
            letter-spacing: 1px !important;
            font-weight: 600 !important;
        }
    """
    
    FAVORITE_BUTTON_FIXES = """
        .agent-actions {
            display: flex;
            align-items: center;
            gap: 4px;
            margin-left: auto;
        }
        
        .favorite-btn {
            background: transparent;
            border: none;
            color: #8a92a5;
            cursor: pointer;
            padding: 4px;
            width: 20px;
            height: 20px;
            border-radius: 4px;
            transition: all 0.3s ease;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 12px;
        }
        
        .favorite-btn:hover {
            color: #FFD700;
            background: rgba(255, 215, 0, 0.1);
        }
        
        .favorite-star {
            display: none !important;
        }
    """
    
    GLASS_SUBMENU_POSITIONING = """
        .glass-submenu {
            position: fixed !important;
            z-index: 1000;
            backdrop-filter: blur(20px);
            background: linear-gradient(135deg, 
                rgba(0, 212, 255, 0.15), 
                rgba(0, 150, 200, 0.1));
            border: 1px solid rgba(0, 212, 255, 0.3);
            border-radius: 12px;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
        }
    """

# Example usage functions
async def fix_users_agents_section():
    """Fix all USERS & AGENTS styling issues"""
    fixes = [
        (CommonCSSFixes.SEARCH_BOX_FIXES, "search box styling"),
        (CommonCSSFixes.TITLE_FIXES, "title typography"),
        (CommonCSSFixes.FAVORITE_BUTTON_FIXES, "favorite button positioning")
    ]
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'section-fixer',
            'agentName': 'Section Fixer', 
            'agentType': 'ai'
        })
        
        fixer = ComponentCSSFixer()
        success_count = 0
        
        for css_rules, description in fixes:
            success = await fixer.apply_css_to_component(
                client, 'agent-selector', css_rules, description
            )
            if success:
                success_count += 1
        
        print(f"\n✅ Applied {success_count}/{len(fixes)} fixes successfully!")
        return success_count == len(fixes)

async def fix_glass_submenu_positioning():
    """Fix glass submenu positioning issues"""
    return await ComponentCSSFixer.quick_style_fix(
        '.glass-submenu',
        CommonCSSFixes.GLASS_SUBMENU_POSITIONING,
        "glass submenu positioning"
    )

if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description='Apply CSS fixes to components')
    parser.add_argument('--users-agents', action='store_true', 
                       help='Fix USERS & AGENTS section styling')
    parser.add_argument('--glass-submenu', action='store_true',
                       help='Fix glass submenu positioning')
    parser.add_argument('--all', action='store_true',
                       help='Apply all common fixes')
    
    args = parser.parse_args()
    
    if args.users_agents or args.all:
        print("🔧 Fixing USERS & AGENTS section...")
        result = asyncio.run(fix_users_agents_section())
        print(f"Result: {'✅ Success' if result else '❌ Failed'}")
    
    if args.glass_submenu or args.all:
        print("🔧 Fixing glass submenu positioning...")
        result = asyncio.run(fix_glass_submenu_positioning())
        print(f"Result: {'✅ Success' if result else '❌ Failed'}")
    
    if not any([args.users_agents, args.glass_submenu, args.all]):
        print("🔧 Component CSS Fixer")
        print("Usage:")
        print("  python component_css_fixer.py --users-agents")
        print("  python component_css_fixer.py --glass-submenu") 
        print("  python component_css_fixer.py --all")