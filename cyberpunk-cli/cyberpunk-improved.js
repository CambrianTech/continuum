/*
 * CYBERPUNK CLI JAVASCRIPT - AI GENERATED IMPROVEMENTS
 * Created by Continuum AI on 2025-06-02T05:36:55.502Z
 */

class CyberpunkCLI {
  constructor() {
    this.initializeTheme();
    this.addPerformanceOptimizations();
  }
  
  initializeTheme() {
    console.log('🤖 AI: Initializing enhanced cyberpunk theme...');
    
    // AI IMPROVEMENT: Better DOM ready detection
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', () => this.applyTheme());
    } else {
      this.applyTheme();
    }
  }
  
  applyTheme() {
    // AI IMPROVEMENT: Apply theme to all CLI elements
    const cliElements = document.querySelectorAll('.terminal, .cli, .cyberpunk, .cyberpunk-terminal');
    
    cliElements.forEach(element => {
      element.classList.add('cyberpunk-terminal', 'cyberpunk-transition');
      
      // AI IMPROVEMENT: Add glow effect to text
      const textNodes = element.querySelectorAll('p, span, div');
      textNodes.forEach(node => {
        if (node.textContent.trim()) {
          node.classList.add('cyberpunk-text');
        }
      });
    });
    
    console.log(`🤖 AI: Enhanced ${cliElements.length} CLI elements`);
  }
  
  // AI IMPROVEMENT: Performance optimization
  addPerformanceOptimizations() {
    // Debounce resize events
    let resizeTimer;
    window.addEventListener('resize', () => {
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        this.handleResize();
      }, 100);
    });
  }
  
  handleResize() {
    // Responsive adjustments are now handled via CSS media queries.
  }
  
  // AI IMPROVEMENT: Add loading animation
  showLoading(element) {
    element.classList.add('cyberpunk-loading');
    setTimeout(() => {
      element.classList.remove('cyberpunk-loading');
    }, 2000);
  }
}

// AI AUTO-INITIALIZATION
if (typeof window !== 'undefined') {
  window.cyberpunkCLI = new CyberpunkCLI();
  console.log('🤖 AI: Cyberpunk CLI enhancements loaded automatically');
}