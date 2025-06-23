/**
 * WebBrowse Command Server Module
 * Exports the WebBrowse command for server-side execution
 */

const WebBrowseCommand = require('./WebBrowseCommand.cjs');

module.exports = {
  command: WebBrowseCommand,
  name: 'webbrowse',
  description: 'Browse websites, take screenshots, and interact with web content using DevTools Protocol'
};