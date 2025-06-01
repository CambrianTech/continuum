#!/usr/bin/env node
/**
 * CLI interface for dev-bridge
 * Allows AI agents to test web apps, take screenshots, and run CLI commands
 */

import { Command } from 'commander';
import chalk from 'chalk';
import { DevBridge } from './dev-bridge';

const program = new Command();

program
  .name('dev-bridge')
  .description('AI development bridge for browser automation and CLI testing')
  .version('0.1.0');

program
  .command('screenshot <url>')
  .description('Take a screenshot of a web page')
  .option('-o, --output <path>', 'Output path for screenshot', './screenshot.png')
  .option('-w, --width <number>', 'Screenshot width', '1280')
  .option('-h, --height <number>', 'Screenshot height', '720')
  .option('--full-page', 'Take full page screenshot')
  .action(async (url, options) => {
    const bridge = new DevBridge({ screenshotDir: './screenshots' });
    
    try {
      await bridge.init();
      
      await bridge.testWebApp(url, {
        screenshot: true,
        screenshotPath: options.output,
        waitTime: 3000
      });
      
      console.log(chalk.green(`✅ Screenshot saved to ${options.output}`));
    } catch (error) {
      console.error(chalk.red(`❌ Error: ${error}`));
      process.exit(1);
    } finally {
      await bridge.close();
    }
  });

program
  .command('test-web <url>')
  .description('Test a web app and capture console logs and errors')
  .option('-s, --screenshot', 'Take screenshot')
  .option('-w, --wait <number>', 'Wait time in milliseconds', '3000')
  .option('--json', 'Output results as JSON')
  .action(async (url, options) => {
    const bridge = new DevBridge({ verboseLogging: !options.json });
    
    try {
      await bridge.init();
      
      const result = await bridge.testWebApp(url, {
        screenshot: options.screenshot,
        waitTime: parseInt(options.wait)
      });
      
      if (options.json) {
        console.log(JSON.stringify(result, null, 2));
      } else {
        console.log(chalk.blue(`🌐 Tested: ${url}`));
        console.log(chalk.yellow(`📊 Console messages: ${result.console.length}`));
        console.log(chalk.red(`❌ JavaScript errors: ${result.errors.length}`));
        
        if (result.errors.length > 0) {
          console.log(chalk.red('\nErrors:'));
          result.errors.forEach(error => console.log(chalk.red(`  ${error}`)));
        }
        
        if (result.console.filter(m => m.type === 'error').length > 0) {
          console.log(chalk.red('\nConsole Errors:'));
          result.console
            .filter(m => m.type === 'error')
            .forEach(msg => console.log(chalk.red(`  ${msg.text}`)));
        }
      }
    } catch (error) {
      console.error(chalk.red(`❌ Error: ${error}`));
      process.exit(1);
    } finally {
      await bridge.close();
    }
  });

program
  .command('debug <url>')
  .description('Debug a web app - comprehensive analysis with screenshot')
  .action(async (url) => {
    const bridge = new DevBridge({ verboseLogging: true });
    
    try {
      await bridge.init();
      
      const result = await bridge.debugWebApp(url);
      
      console.log(chalk.blue(`🔍 Debug Analysis for: ${url}`));
      console.log(chalk.yellow(`📸 Screenshot: ${result.screenshot}`));
      
      if (result.jsErrors.length > 0) {
        console.log(chalk.red('\n🚨 JavaScript Errors:'));
        result.jsErrors.forEach(error => console.log(chalk.red(`  ${error}`)));
      }
      
      if (result.consoleErrors.length > 0) {
        console.log(chalk.red('\n📝 Console Errors:'));
        result.consoleErrors.forEach(error => console.log(chalk.red(`  ${error}`)));
      }
      
      if (result.performanceIssues.length > 0) {
        console.log(chalk.yellow('\n⚡ Performance Issues:'));
        result.performanceIssues.forEach(issue => console.log(chalk.yellow(`  ${issue}`)));
      }
      
      if (result.jsErrors.length === 0 && result.consoleErrors.length === 0) {
        console.log(chalk.green('\n✅ No errors detected!'));
      }
      
    } catch (error) {
      console.error(chalk.red(`❌ Error: ${error}`));
      process.exit(1);
    } finally {
      await bridge.close();
    }
  });

program
  .command('run-cli <command>')
  .description('Run a CLI command and capture output')
  .option('-c, --cwd <path>', 'Working directory')
  .option('-t, --timeout <number>', 'Timeout in milliseconds', '30000')
  .option('--json', 'Output results as JSON')
  .action(async (command, options) => {
    const bridge = new DevBridge();
    
    try {
      await bridge.init();
      
      const result = await bridge.cliRunner.testCommand(command, {
        cwd: options.cwd,
        timeout: parseInt(options.timeout),
        shell: true
      });
      
      if (options.json) {
        console.log(JSON.stringify(result, null, 2));
      } else {
        console.log(chalk.blue(`🖥️  Command: ${result.command}`));
        console.log(chalk.yellow(`⏱️  Duration: ${result.duration}ms`));
        console.log(chalk.green(`🔢 Exit Code: ${result.exitCode}`));
        
        if (result.stdout) {
          console.log(chalk.blue('\n📤 STDOUT:'));
          console.log(result.stdout);
        }
        
        if (result.stderr) {
          console.log(chalk.red('\n📥 STDERR:'));
          console.log(result.stderr);
        }
      }
      
    } catch (error) {
      console.error(chalk.red(`❌ Error: ${error}`));
      process.exit(1);
    } finally {
      await bridge.close();
    }
  });

program
  .command('compare <python-script> <web-url>')
  .description('Compare Python CLI output with web demo')
  .option('-a, --args <args>', 'Arguments for Python script')
  .option('-w, --wait <number>', 'Wait time for web test', '3000')
  .option('-c, --cwd <path>', 'Working directory for Python script')
  .action(async (pythonScript, webUrl, options) => {
    const bridge = new DevBridge({ verboseLogging: true });
    
    try {
      await bridge.init();
      
      const args = options.args ? options.args.split(' ') : [];
      
      const result = await bridge.compareImplementations({
        pythonCLI: {
          script: pythonScript,
          args,
          cwd: options.cwd
        },
        webDemo: {
          url: webUrl,
          waitTime: parseInt(options.wait)
        },
        screenshotBaseName: 'comparison'
      });
      
      console.log(chalk.blue('🔍 Implementation Comparison'));
      console.log(chalk.yellow('\n🐍 Python CLI Results:'));
      console.log(`Exit Code: ${result.python.exitCode}`);
      console.log(`Duration: ${result.python.duration}ms`);
      if (result.python.stdout) {
        console.log('STDOUT:', result.python.stdout.substring(0, 200));
      }
      
      console.log(chalk.yellow('\n🌐 Web Demo Results:'));
      console.log(`Errors: ${result.web.errors.length}`);
      console.log(`Console Messages: ${result.web.console.length}`);
      
      if (result.screenshots) {
        console.log(chalk.green(`\n📸 Screenshots saved:`));
        console.log(`Web: ${result.screenshots.web}`);
      }
      
    } catch (error) {
      console.error(chalk.red(`❌ Error: ${error}`));
      process.exit(1);
    } finally {
      await bridge.close();
    }
  });

program.parse();