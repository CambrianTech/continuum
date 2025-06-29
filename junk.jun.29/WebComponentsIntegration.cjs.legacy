/**
 * Web Components Integration
 * Gradually introduces web components into the existing UI without breaking it
 */

class WebComponentsIntegration {
  constructor() {
    this.componentsToInject = [
      {
        selector: '.user-selector',
        component: 'user-selector',
        scriptPath: '/src/ui/components/UserSelector/UserSelector.js'
      },
      {
        selector: '.saved-personas',
        component: 'saved-personas',
        scriptPath: '/src/ui/components/SavedPersonas/SavedPersonas.js'
      },
      {
        selector: '.active-projects',
        component: 'active-projects',
        scriptPath: '/src/ui/components/ActiveProjects/ActiveProjects.js'
      }
    ];
  }

  /**
   * Get component script tags for injection
   */
  getComponentScripts() {
    return this.componentsToInject
      .map(comp => `<script src="${comp.scriptPath}"></script>`)
      .join('\n    ');
  }

  /**
   * Get initialization script for progressive enhancement
   */
  getInitializationScript() {
    return `
      <script>
        // Progressive enhancement - gradually replace HTML with web components
        document.addEventListener('DOMContentLoaded', function() {
          // Wait for components to load
          setTimeout(() => {
            progressivelyEnhanceUI();
          }, 100);
        });

        function progressivelyEnhanceUI() {
          // Replace user selector if it exists
          const userSelector = document.querySelector('.user-selector');
          if (userSelector && typeof customElements !== 'undefined' && customElements.get('user-selector')) {
            try {
              const newComponent = document.createElement('user-selector');
              
              // Copy any existing state/data
              const existingData = extractUserSelectorData(userSelector);
              if (existingData) {
                newComponent.agents = existingData.agents;
                newComponent.selectedAgent = existingData.selectedAgent;
              }
              
              // Replace the old element
              userSelector.parentNode.replaceChild(newComponent, userSelector);
              console.log('✅ Enhanced user selector with web component');
            } catch (error) {
              console.warn('Failed to enhance user selector:', error);
            }
          }

          // Replace saved personas if it exists
          const savedPersonas = document.querySelector('.saved-personas');
          if (savedPersonas && typeof customElements !== 'undefined' && customElements.get('saved-personas')) {
            try {
              const newComponent = document.createElement('saved-personas');
              
              // Replace the old element
              savedPersonas.parentNode.replaceChild(newComponent, savedPersonas);
              console.log('✅ Enhanced saved personas with web component');
            } catch (error) {
              console.warn('Failed to enhance saved personas:', error);
            }
          }

          // Replace active projects if it exists
          const activeProjects = document.querySelector('.active-projects');
          if (activeProjects && typeof customElements !== 'undefined' && customElements.get('active-projects')) {
            try {
              const newComponent = document.createElement('active-projects');
              
              // Replace the old element
              activeProjects.parentNode.replaceChild(newComponent, activeProjects);
              console.log('✅ Enhanced active projects with web component');
            } catch (error) {
              console.warn('Failed to enhance active projects:', error);
            }
          }
        }

        function extractUserSelectorData(element) {
          // Extract data from existing HTML structure
          const agents = [];
          const agentItems = element.querySelectorAll('.agent-item');
          
          agentItems.forEach(item => {
            const name = item.querySelector('.agent-name')?.textContent;
            const role = item.querySelector('.agent-role')?.textContent;
            const avatar = item.querySelector('.agent-avatar')?.textContent;
            const isSelected = item.classList.contains('selected');
            
            if (name) {
              agents.push({
                id: name.toLowerCase().replace(/\s+/g, ''),
                name: name,
                role: role || '',
                avatar: avatar || '🤖',
                status: 'online',
                type: 'ai'
              });
              
              if (isSelected) {
                selectedAgent = agents[agents.length - 1].id;
              }
            }
          });
          
          return agents.length > 0 ? { agents, selectedAgent: selectedAgent || 'auto' } : null;
        }
      </script>
    `;
  }

  /**
   * Inject components into existing HTML
   */
  enhanceHTML(html) {
    // Add component scripts before closing body tag
    const scriptsToAdd = this.getComponentScripts();
    const initScript = this.getInitializationScript();
    
    // Insert scripts before closing body tag
    const enhancedHTML = html.replace(
      '</body>',
      `    ${scriptsToAdd}\n    ${initScript}\n</body>`
    );

    return enhancedHTML;
  }
}

module.exports = WebComponentsIntegration;