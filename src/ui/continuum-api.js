/**
 * Continuum Web API
 * Provides unified interface for Continuum web functionality
 */

window.continuum = {
    version: '0.2.1913',
    
    api: {
        screenshot: {
            take: function(name = 'proof') {
                const timestamp = Date.now();
                const filename = `${name}-${timestamp}.png`;
                console.log(`📸 Taking screenshot: ${filename}`);
                if (window.screenshotSystem) {
                    window.screenshotSystem.takeScreenshot(filename);
                } else {
                    console.warn('❌ Screenshot system not available');
                }
                return filename;
            }
        },
        
        ui: {
            addDrawerButtons: function() {
                console.log('🔧 Adding drawer buttons to users...');
                const userItems = document.querySelectorAll('.agent-item, .user-item');
                userItems.forEach(item => {
                    if (!item.querySelector('.drawer-btn')) {
                        const drawerBtn = document.createElement('span');
                        drawerBtn.className = 'drawer-btn';
                        drawerBtn.textContent = '⬢';
                        drawerBtn.style.cssText = `
                            margin-left: auto;
                            padding: 4px 8px;
                            background: rgba(79, 195, 247, 0.2);
                            border: 1px solid rgba(79, 195, 247, 0.4);
                            border-radius: 4px;
                            cursor: pointer;
                            font-size: 12px;
                            color: #4FC3F7;
                            transition: all 0.2s ease;
                        `;
                        drawerBtn.addEventListener('click', () => {
                            console.log('🎯 Drawer clicked for:', item.textContent);
                        });
                        item.appendChild(drawerBtn);
                    }
                });
            },
            
            refreshUsers: function() {
                console.log('🔄 Refreshing user section...');
                this.addDrawerButtons();
            }
        },
        
        debug: {
            takeProofScreenshot: function() {
                return window.continuum.api.screenshot.take('continuum-proof');
            },
            
            info: function() {
                console.log('🔧 Continuum Debug Info:', {
                    version: window.continuum.version,
                    websocket: !!window.ws,
                    screenshot: !!window.screenshotSystem,
                    components: !!window.componentSystemReady
                });
            }
        }
    }
};

console.log('✅ window.continuum API initialized');