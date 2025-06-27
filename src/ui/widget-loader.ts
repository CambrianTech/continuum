/**
 * Widget Loader - Automatically imports and registers all TypeScript widgets
 * Drop-in functionality for the sidebar and main UI
 */

// Import all TypeScript widgets
import './components/Sidebar/SidebarWidget.js';
import './components/SavedPersonas/SavedPersonas.js';
import './components/UserSelector/UserSelector.js'; 
import './components/ActiveProjects/ActiveProjects.js';
import './components/Chat/ChatWidget.js';

console.log('🎛️ Widget Loader: All TypeScript widgets loaded and registered');

// Export a simple function to verify loading
export function verifyWidgets(): boolean {
  const widgets = [
    'continuum-sidebar',
    'saved-personas', 
    'user-selector',
    'active-projects',
    'chat-widget'
  ];

  const registered = widgets.filter(name => customElements.get(name));
  console.log(`🎛️ Widget Loader: ${registered.length}/${widgets.length} widgets registered:`, registered);
  
  return registered.length === widgets.length;
}