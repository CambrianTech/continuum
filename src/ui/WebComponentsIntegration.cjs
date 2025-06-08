/**
 * Web Components Integration
 * Gradually introduces web components into the existing UI without breaking it
 */

class WebComponentsIntegration {
  constructor() {
    this.componentsToInject = [
      {
        selector: '.agent-selector',
        component: 'agent-selector',
        scriptPath: '/src/ui/components/AgentSelector.js'
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
          // Replace agent selector if it exists
          const agentSelector = document.querySelector('.agent-selector');
          if (agentSelector && typeof customElements !== 'undefined' && customElements.get('agent-selector')) {
            try {
              const newComponent = document.createElement('agent-selector');
              
              // Copy any existing state/data
              const existingData = extractAgentSelectorData(agentSelector);
              if (existingData) {
                newComponent.agents = existingData.agents;
                newComponent.selectedAgent = existingData.selectedAgent;
              }
              
              // Replace the old element
              agentSelector.parentNode.replaceChild(newComponent, agentSelector);
              console.log('✅ Enhanced agent selector with web component');
            } catch (error) {
              console.warn('Failed to enhance agent selector:', error);
            }
          }
        }

        function extractAgentSelectorData(element) {
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