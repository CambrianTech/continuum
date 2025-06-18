# 📁 Continuum Codebase Structure

> **🏛️ Archaeological Discovery**: Advanced AI platform with hierarchical LoRA specialization system  
> **📋 Mission**: Document all capabilities and restore lost treasures from git history

## 🔍 **Agent Study Guide - Archaeological Discoveries**

### 🎓 **DISCOVERY: Global Competitive AI Ecosystem** 
**Found in**: `src/adapters/HierarchicalAdapter.cjs`, `src/commands/core/share/ShareCommand.cjs`, Academy scoring system

**🏅 Global AI Competition Network**:
- **Every Continuum is connected** - Share button publishes to global network
- **AI Gyms & Leaderboards** - Specialists compete globally with Academy Scores
- **Piecemeal benchmarking** - Load competing specialists for head-to-head testing
- **Evolutionary pressure** - Best performers spread, weak ones fade away

**🧠 Composable Expertise Architecture**:
- **Base → Science → Physics → Biology** layered specialization
- **190,735x storage reduction** (29MB adapters vs 175GB base models)
- **Torrent-style sharing** of expertise layers between organizations
- **Dynamic assembly** of AI specialists (legal → patent → uspto → biotech)

**💡 Real Examples Found**:
```
Legal Stack: base → legal(12MB) → patent(8MB) → uspto(5MB) → biotech(4MB) = 29MB total
Medical Stack: base → medical(15MB) → cardiology(10MB) → pediatric(6MB) = 31MB total  
Engineering: base → engineering(11MB) → software(9MB) → ai(7MB) = 27MB total
```

**🚀 Revolutionary Capabilities**:
- **Instant specialization swapping** - Change expertise in seconds, not hours
- **Decentralized expertise mesh** - Organizations share tiny adaptation layers
- **Privacy-preserving** - Base models stay private, only improvements shared
- **Academy-trained layers** - TestingDroid vs ProtocolSheriff creates specialists

### 🏗️ **System Architecture Overview**
**Working Foundation**: 35 modular commands + complete automation + Academy system
**Lost Treasures**: Mass Effect UI, intelligent routing, browser automation integration
**Recovery Status**: All components recoverable from git history with exact commands

## 🎯 Structure Goals
- **Minimize depth** - Flatten nested directories where possible
- **Clear naming** - Every file name should explain its purpose  
- **Consolidate related** - Group similar functionality together
- **Remove dead code** - Delete unused or redundant files

## 📋 File Tree with Agent Comments

```
**File structure overview (detailed analysis in sections below)**

.
├── __tests__
│   ├── comprehensive
│   │   └── system-integration
│   │       ├── complete_system_test.py
│   │       ├── current_system_test.py
│   │       └── FullSystemIntegration.test.cjs
│   ├── config
│   │   ├── jest.config.cjs
│   │   ├── jest.global-setup.js
│   │   ├── jest.global-teardown.js
│   │   ├── pytest.ini
│   │   └── test-runner.cjs
│   ├── critical
│   │   └── core-functionality
│   │       └── ActualScreenshotCreation.test.cjs
│   ├── fixtures
│   │   ├── configs
│   │   ├── data
│   │   └── mocks
│   ├── functional
│   │   ├── user-scenarios
│   │   │   └── WidgetIterationTests.test.js
│   │   ├── visual
│   │   │   └── VisualControlModule.test.js
│   │   └── workflows
│   │       └── CommandIntegrationTests.test.js
│   ├── integration
│   │   ├── ai
│   │   │   └── AICapabilities.test.js
│   │   ├── api
│   │   │   ├── AcademyPersistentStorage.test.cjs
│   │   │   ├── conftest.py
│   │   │   ├── test_browser_api_direct.py
│   │   │   ├── test_crash_recovery.py
│   │   │   ├── test_elegant_api.py
│   │   │   ├── test_elegant_browser_api.py
│   │   │   ├── test_fred_registration.py
│   │   │   ├── test_full_flow.py
│   │   │   ├── test_html_parsing.py
│   │   │   ├── test_js_promise_errors.py
│   │   │   ├── test_promise_flow.py
│   │   │   └── test_ui_updates.py
│   │   ├── commands
│   │   │   ├── ModularCommandSystem.test.cjs
│   │   │   ├── test_modular_commands.py
│   │   │   └── test_validate_code_command.py
│   │   ├── screenshot
│   │   │   ├── full-screen-capture.test.py
│   │   │   ├── screenshot-pipeline.test.py
│   │   │   ├── ScreenshotIntegration.test.cjs
│   │   │   ├── test_screenshot_bytes_mode.py
│   │   │   ├── test_screenshot_simple.py
│   │   │   ├── test_screenshot.py
│   │   │   ├── whole-screen-capture.test.py
│   │   │   └── widget-capture.test.py
│   │   ├── ui
│   │   │   ├── GroupChat.test.js
│   │   │   ├── test_ignoreelements_fix.py
│   │   │   ├── test_permanent_fix.py
│   │   │   ├── test_users_widget.py
│   │   │   ├── test_validation_fix.py
│   │   │   └── UIComponents.test.js
│   │   ├── websocket
│   │   │   └── test_connection.py
│   │   └── widgets
│   │       └── WebSocketSync.test.js
│   ├── python
│   │   └── screenshot-client.py
│   ├── unit
│   │   ├── js
│   │   │   ├── commands
│   │   │   ├── components
│   │   │   ├── core
│   │   │   └── utils
│   │   └── python
│   │       ├── client
│   │       ├── core
│   │       └── utils
│   ├── command-dependency-sort.cjs
│   ├── dependency-aware-test-runner.cjs
│   ├── README.md
│   ├── run-python-tests.cjs
│   ├── scan-command-dependencies.cjs
│   ├── setup.js
│   ├── simple-test-runner.cjs
│   ├── test-dependency-sorting.cjs
│   └── test-strategy.md
├── agent-scripts
│   ├── bin
│   │   ├── heal
│   │   ├── health-monitor
│   │   ├── js-send
│   │   ├── probe
│   │   ├── run-with-venv.py
│   │   └── smart-heal
│   ├── docs
│   │   ├── ARCHITECTURE.md
│   │   ├── CONTRIBUTING.md
│   │   ├── EXAMPLES.md


## 📝 Detailed File Analysis

### Root Directory

### ADVERSARIAL_ROADMAP.md {#adversarial_roadmap.md}
  # ⚔️ ADVERSARIAL ROADMAP: GAN-inspired dual-LLM architecture - ProtocolSheriff vs TestingDroid, CodeCritic vs CodeGenerator, Meta-adversarial systems for self-improving quality

### ARCHITECTURE.md {#architecture.md}
  # 📖 Documentation

### babel.config.cjs {#babel.config.cjs}
  # ⚡ JavaScript/Node.js

### CHECKIN_SUMMARY.md {#checkin_summary.md}
  # 📖 Documentation

### continuum.cjs {#continuum.cjs}
  # ⚡ JavaScript/Node.js

### continuum.log {#continuum.log}
  # 📄 File

### eslint.config.js {#eslint.config.js}
  # ⚡ JavaScript/Node.js

### files_temp.md {#files_temp.md}
  # 📖 Documentation

### FILES.md {#files.md}
  # 📖 Documentation

### FILES.md.backup {#files.md.backup}
  # 📄 File

### FILES.md.test-backup {#files.md.test-backup}
  # 📄 File

### increment-version.js {#increment-version.js}
  # ⚡ JavaScript/Node.js

### jest.config.cjs {#jest.config.cjs}
  # ⚡ JavaScript/Node.js

### jest.config.ui.js {#jest.config.ui.js}
  # ⚡ JavaScript/Node.js

### lerna.json {#lerna.json}
  # 📋 Configuration/Data

### message-to-ai.json {#message-to-ai.json}
  # 📋 Configuration/Data

### package-lock.json {#package-lock.json}
  # 📋 Configuration/Data

### package.json {#package.json}
  # 📋 Configuration/Data

### process.md {#process.md}
  # 📖 Documentation

### README-CLEAN.md {#readme-clean.md}
  # 📖 Documentation

### README-UPDATED.md {#readme-updated.md}
  # 📖 Documentation

### README.md {#readme.md}
  # 📖 Documentation

### RESTORATION-STRATEGY.md {#restoration-strategy.md}
  # 📖 Documentation

### ROADMAP.md {#roadmap.md}
  # 📖 Documentation

### SCREENSHOT_REFERENCE.md {#screenshot_reference.md}
  # 📖 Documentation

### server.log {#server.log}
  # 📄 File

### tsconfig.json {#tsconfig.json}
  # 📋 Configuration/Data

### tsconfig.ui.json {#tsconfig.ui.json}
  # 📋 Configuration/Data

### WORKING_NOTES.md {#working_notes.md}
  # 📖 Documentation


📁 **__tests__/**
### command-dependency-sort.cjs {#__tests__-command-dependency-sort.cjs}
  # ⚡ JavaScript/Node.js

### dependency-aware-test-runner.cjs {#__tests__-dependency-aware-test-runner.cjs}
  # ⚡ JavaScript/Node.js

### README.md {#__tests__-readme.md}
  # 📖 Documentation

### run-python-tests.cjs {#__tests__-run-python-tests.cjs}
  # ⚡ JavaScript/Node.js

### scan-command-dependencies.cjs {#__tests__-scan-command-dependencies.cjs}
  # ⚡ JavaScript/Node.js

### setup.js {#__tests__-setup.js}
  # ⚡ JavaScript/Node.js

### simple-test-runner.cjs {#__tests__-simple-test-runner.cjs}
  # ⚡ JavaScript/Node.js

### test-dependency-sorting.cjs {#__tests__-test-dependency-sorting.cjs}
  # ⚡ JavaScript/Node.js

### test-strategy.md {#__tests__-test-strategy.md}
  # 📖 Documentation

  📁 **__tests__/comprehensive/**
    📁 **__tests__/comprehensive/system-integration/**
    ### complete_system_test.py {#__tests__-comprehensive-system-integration-complete_system_test.py}
      # 🐍 Python

    ### current_system_test.py {#__tests__-comprehensive-system-integration-current_system_test.py}
      # 🐍 Python

    ### FullSystemIntegration.test.cjs {#__tests__-comprehensive-system-integration-fullsystemintegration.test.cjs}
      # ⚡ JavaScript/Node.js

  📁 **__tests__/config/**
  ### jest.config.cjs {#__tests__-config-jest.config.cjs}
    # ⚡ JavaScript/Node.js

  ### jest.global-setup.js {#__tests__-config-jest.global-setup.js}
    # ⚡ JavaScript/Node.js

  ### jest.global-teardown.js {#__tests__-config-jest.global-teardown.js}
    # ⚡ JavaScript/Node.js

  ### pytest.ini {#__tests__-config-pytest.ini}
    # 📄 File

  ### test-runner.cjs {#__tests__-config-test-runner.cjs}
    # ⚡ JavaScript/Node.js

  📁 **__tests__/critical/**
    📁 **__tests__/critical/core-functionality/**
    ### ActualScreenshotCreation.test.cjs {#__tests__-critical-core-functionality-actualscreenshotcreation.test.cjs}
      # ⚡ JavaScript/Node.js

  📁 **__tests__/fixtures/**
    📁 **__tests__/fixtures/configs/**
    📁 **__tests__/fixtures/data/**
    📁 **__tests__/fixtures/mocks/**
  📁 **__tests__/functional/**
    📁 **__tests__/functional/user-scenarios/**
    ### WidgetIterationTests.test.js {#__tests__-functional-user-scenarios-widgetiterationtests.test.js}
      # ⚡ JavaScript/Node.js

    📁 **__tests__/functional/visual/**
    ### VisualControlModule.test.js {#__tests__-functional-visual-visualcontrolmodule.test.js}
      # ⚡ JavaScript/Node.js

    📁 **__tests__/functional/workflows/**
    ### CommandIntegrationTests.test.js {#__tests__-functional-workflows-commandintegrationtests.test.js}
      # ⚡ JavaScript/Node.js

  📁 **__tests__/integration/**
    📁 **__tests__/integration/ai/**
    ### AICapabilities.test.js {#__tests__-integration-ai-aicapabilities.test.js}
      # ⚡ JavaScript/Node.js

    📁 **__tests__/integration/api/**
    ### AcademyPersistentStorage.test.cjs {#__tests__-integration-api-academypersistentstorage.test.cjs}
      # ⚡ JavaScript/Node.js

    ### conftest.py {#__tests__-integration-api-conftest.py}
      # 🐍 Python

    ### test_browser_api_direct.py {#__tests__-integration-api-test_browser_api_direct.py}
      # 🐍 Python

    ### test_crash_recovery.py {#__tests__-integration-api-test_crash_recovery.py}
      # 🐍 Python

    ### test_elegant_api.py {#__tests__-integration-api-test_elegant_api.py}
      # 🐍 Python

    ### test_elegant_browser_api.py {#__tests__-integration-api-test_elegant_browser_api.py}
      # 🐍 Python

    ### test_fred_registration.py {#__tests__-integration-api-test_fred_registration.py}
      # 🐍 Python

    ### test_full_flow.py {#__tests__-integration-api-test_full_flow.py}
      # 🐍 Python

    ### test_html_parsing.py {#__tests__-integration-api-test_html_parsing.py}
      # 🐍 Python

    ### test_js_promise_errors.py {#__tests__-integration-api-test_js_promise_errors.py}
      # 🐍 Python

    ### test_promise_flow.py {#__tests__-integration-api-test_promise_flow.py}
      # 🐍 Python

    ### test_ui_updates.py {#__tests__-integration-api-test_ui_updates.py}
      # 🐍 Python

    📁 **__tests__/integration/commands/**
    ### ModularCommandSystem.test.cjs {#__tests__-integration-commands-modularcommandsystem.test.cjs}
      # ⚡ JavaScript/Node.js

    ### test_modular_commands.py {#__tests__-integration-commands-test_modular_commands.py}
      # 🐍 Python

    ### test_validate_code_command.py {#__tests__-integration-commands-test_validate_code_command.py}
      # 🐍 Python

    📁 **__tests__/integration/screenshot/**
    ### full-screen-capture.test.py {#__tests__-integration-screenshot-full-screen-capture.test.py}
      # 🐍 Python

    ### screenshot-pipeline.test.py {#__tests__-integration-screenshot-screenshot-pipeline.test.py}
      # 🐍 Python

    ### ScreenshotIntegration.test.cjs {#__tests__-integration-screenshot-screenshotintegration.test.cjs}
      # ⚡ JavaScript/Node.js

    ### test_screenshot_bytes_mode.py {#__tests__-integration-screenshot-test_screenshot_bytes_mode.py}
      # 🐍 Python

    ### test_screenshot_simple.py {#__tests__-integration-screenshot-test_screenshot_simple.py}
      # 🐍 Python

    ### test_screenshot.py {#__tests__-integration-screenshot-test_screenshot.py}
      # 🐍 Python

    ### whole-screen-capture.test.py {#__tests__-integration-screenshot-whole-screen-capture.test.py}
      # 🐍 Python

    ### widget-capture.test.py {#__tests__-integration-screenshot-widget-capture.test.py}
      # 🐍 Python

    📁 **__tests__/integration/ui/**
    ### GroupChat.test.js {#__tests__-integration-ui-groupchat.test.js}
      # ⚡ JavaScript/Node.js

    ### test_ignoreelements_fix.py {#__tests__-integration-ui-test_ignoreelements_fix.py}
      # 🐍 Python

    ### test_permanent_fix.py {#__tests__-integration-ui-test_permanent_fix.py}
      # 🐍 Python

    ### test_users_widget.py {#__tests__-integration-ui-test_users_widget.py}
      # 🐍 Python

    ### test_validation_fix.py {#__tests__-integration-ui-test_validation_fix.py}
      # 🐍 Python

    ### UIComponents.test.js {#__tests__-integration-ui-uicomponents.test.js}
      # ⚡ JavaScript/Node.js

    📁 **__tests__/integration/websocket/**
    ### test_connection.py {#__tests__-integration-websocket-test_connection.py}
      # 🐍 Python

    📁 **__tests__/integration/widgets/**
    ### WebSocketSync.test.js {#__tests__-integration-widgets-websocketsync.test.js}
      # ⚡ JavaScript/Node.js

  📁 **__tests__/python/**
  ### screenshot-client.py {#__tests__-python-screenshot-client.py}
    # 🐍 Python

  📁 **__tests__/unit/**
    📁 **__tests__/unit/js/**
      📁 **__tests__/unit/js/commands/**
      ### CommandProcessor.test.cjs {#__tests__-unit-js-commands-commandprocessor.test.cjs}
        # ⚡ JavaScript/Node.js

      ### CommandStreamer.test.cjs {#__tests__-unit-js-commands-commandstreamer.test.cjs}
        # ⚡ JavaScript/Node.js

      ### README-DrivenHelp.test.cjs {#__tests__-unit-js-commands-readme-drivenhelp.test.cjs}
        # ⚡ JavaScript/Node.js

      📁 **__tests__/unit/js/components/**
      ### CyberpunkDrawer.test.cjs {#__tests__-unit-js-components-cyberpunkdrawer.test.cjs}
        # ⚡ JavaScript/Node.js

      ### ScreenshotFeedback.test.cjs {#__tests__-unit-js-components-screenshotfeedback.test.cjs}
        # ⚡ JavaScript/Node.js

      ### UIModular.test.cjs {#__tests__-unit-js-components-uimodular.test.cjs}
        # ⚡ JavaScript/Node.js

      📁 **__tests__/unit/js/core/**
      ### basic-structure.test.js {#__tests__-unit-js-core-basic-structure.test.js}
        # ⚡ JavaScript/Node.js

      ### JavaScriptValidation.test.cjs {#__tests__-unit-js-core-javascriptvalidation.test.cjs}
        # ⚡ JavaScript/Node.js

      ### PersistentStorage.test.cjs {#__tests__-unit-js-core-persistentstorage.test.cjs}
        # ⚡ JavaScript/Node.js

      ### ProtocolSheriff.test.cjs {#__tests__-unit-js-core-protocolsheriff.test.cjs}
        # ⚡ JavaScript/Node.js

      ### storage-basic.test.js {#__tests__-unit-js-core-storage-basic.test.js}
        # ⚡ JavaScript/Node.js

      ### VersionManagement.test.cjs {#__tests__-unit-js-core-versionmanagement.test.cjs}
        # ⚡ JavaScript/Node.js

      📁 **__tests__/unit/js/utils/**
      ### ContinuonPositioning.simple.test.cjs {#__tests__-unit-js-utils-continuonpositioning.simple.test.cjs}
        # ⚡ JavaScript/Node.js

      ### ContinuonPositioning.test.cjs {#__tests__-unit-js-utils-continuonpositioning.test.cjs}
        # ⚡ JavaScript/Node.js

      ### ImportValidation.test.cjs {#__tests__-unit-js-utils-importvalidation.test.cjs}
        # ⚡ JavaScript/Node.js

      ### PromiseBasedAPI.test.cjs {#__tests__-unit-js-utils-promisebasedapi.test.cjs}
        # ⚡ JavaScript/Node.js

      ### WebSocketStreaming.test.cjs {#__tests__-unit-js-utils-websocketstreaming.test.cjs}
        # ⚡ JavaScript/Node.js

    📁 **__tests__/unit/python/**
      📁 **__tests__/unit/python/client/**
      ### test_client.py {#__tests__-unit-python-client-test_client.py}
        # 🐍 Python

      ### test_js_executor.py {#__tests__-unit-python-client-test_js_executor.py}
        # 🐍 Python

      ### test_screenshot_utils.py {#__tests__-unit-python-client-test_screenshot_utils.py}
        # 🐍 Python

      📁 **__tests__/unit/python/core/**
      ### test_app_store_validation.py {#__tests__-unit-python-core-test_app_store_validation.py}
        # 🐍 Python

      ### test_basic_structure.py {#__tests__-unit-python-core-test_basic_structure.py}
        # 🐍 Python

      ### test_simple_js.py {#__tests__-unit-python-core-test_simple_js.py}
        # 🐍 Python

      📁 **__tests__/unit/python/utils/**

📁 **agent-scripts/**
### activate-env.sh {#agent-scripts-activate-env.sh}
  # 🔧 Shell Script

### DIRECTORY_STRUCTURE.md {#agent-scripts-directory_structure.md}
  # 📖 Documentation

### README.md {#agent-scripts-readme.md}
  # 📖 Documentation

### requirements.txt {#agent-scripts-requirements.txt}
  # 📦 Python dependencies

  📁 **agent-scripts/bin/**
  ### heal {#agent-scripts-bin-heal}
    # 📄 File

  ### health-monitor {#agent-scripts-bin-health-monitor}
    # 📄 File

  ### js-send {#agent-scripts-bin-js-send}
    # 📄 File

  ### probe {#agent-scripts-bin-probe}
    # 📄 File

  ### run-with-venv.py {#agent-scripts-bin-run-with-venv.py}
    # 🐍 Python

  ### smart-heal {#agent-scripts-bin-smart-heal}
    # 📄 File

  📁 **agent-scripts/docs/**
  ### ARCHITECTURE.md {#agent-scripts-docs-architecture.md}
    # 📖 Documentation

  ### CONTRIBUTING.md {#agent-scripts-docs-contributing.md}
    # 📖 Documentation

  ### EXAMPLES.md {#agent-scripts-docs-examples.md}
    # 📖 Documentation

  ### USER_KINDNESS.md {#agent-scripts-docs-user_kindness.md}
    # 📖 Documentation

  📁 **agent-scripts/examples/**
    📁 **agent-scripts/examples/diagnostics/**
    ### console-probe.js {#agent-scripts-examples-diagnostics-console-probe.js}
      # ⚡ JavaScript/Node.js

    ### error-capture.js {#agent-scripts-examples-diagnostics-error-capture.js}
      # ⚡ JavaScript/Node.js

    ### full-system-check.js {#agent-scripts-examples-diagnostics-full-system-check.js}
      # ⚡ JavaScript/Node.js

    ### joke-delivery-test.js {#agent-scripts-examples-diagnostics-joke-delivery-test.js}
      # ⚡ JavaScript/Node.js

    ### live-browser-investigation.js {#agent-scripts-examples-diagnostics-live-browser-investigation.js}
      # ⚡ JavaScript/Node.js

    ### probe-test.js {#agent-scripts-examples-diagnostics-probe-test.js}
      # ⚡ JavaScript/Node.js

    ### test-script.js {#agent-scripts-examples-diagnostics-test-script.js}
      # ⚡ JavaScript/Node.js

    📁 **agent-scripts/examples/fixes/**
    ### auto-repair.js {#agent-scripts-examples-fixes-auto-repair.js}
      # ⚡ JavaScript/Node.js

    ### comprehensive-fix.js {#agent-scripts-examples-fixes-comprehensive-fix.js}
      # ⚡ JavaScript/Node.js

    ### websocket-fix.js {#agent-scripts-examples-fixes-websocket-fix.js}
      # ⚡ JavaScript/Node.js

    📁 **agent-scripts/examples/jokes/**
    ### ai-joke.js {#agent-scripts-examples-jokes-ai-joke.js}
      # ⚡ JavaScript/Node.js

    ### css-joke.js {#agent-scripts-examples-jokes-css-joke.js}
      # ⚡ JavaScript/Node.js

    ### self-healing-demo.js {#agent-scripts-examples-jokes-self-healing-demo.js}
      # ⚡ JavaScript/Node.js

    ### tooth-joke.js {#agent-scripts-examples-jokes-tooth-joke.js}
      # ⚡ JavaScript/Node.js

  📁 **agent-scripts/tools/**
    📁 **agent-scripts/tools/javascript/**
    📁 **agent-scripts/tools/python/**
    ### heal.py {#agent-scripts-tools-python-heal.py}
      # 🐍 Python

    ### health-monitor.py {#agent-scripts-tools-python-health-monitor.py}
      # 🐍 Python

    ### js-send-http-legacy.py {#agent-scripts-tools-python-js-send-http-legacy.py}
      # 🐍 Python

    ### js-send.py {#agent-scripts-tools-python-js-send.py}
      # 🐍 Python

    ### probe-safe.py {#agent-scripts-tools-python-probe-safe.py}
      # 🐍 Python

    ### setup.py {#agent-scripts-tools-python-setup.py}
      # 🐍 Python

    ### smart-heal.py {#agent-scripts-tools-python-smart-heal.py}
      # 🐍 Python


📁 **agents/**
  📁 **agents/workspace/**
  ### advanced_boot_validator.py {#agents-workspace-advanced_boot_validator.py}
    # 🐍 Python

  ### CLAUDE_BUS_FEATURES.md {#agents-workspace-claude_bus_features.md}
    # 📖 Documentation

  ### claude_bus_validation_command.js {#agents-workspace-claude_bus_validation_command.js}
    # ⚡ JavaScript/Node.js

  ### claude_debug_session.js {#agents-workspace-claude_debug_session.js}
    # ⚡ JavaScript/Node.js

  ### client_debug_workflow.py {#agents-workspace-client_debug_workflow.py}
    # 🐍 Python

  ### ClientConnection.js {#agents-workspace-clientconnection.js}
    # ⚡ JavaScript/Node.js

  ### ClientConnection.py {#agents-workspace-clientconnection.py}
    # 🐍 Python

  ### communication_validator.py {#agents-workspace-communication_validator.py}
    # 🐍 Python

  ### core_boot_validator.py {#agents-workspace-core_boot_validator.py}
    # 🐍 Python

  ### debug_screenshot_console.js {#agents-workspace-debug_screenshot_console.js}
    # ⚡ JavaScript/Node.js

  ### fix_websocket_connection.js {#agents-workspace-fix_websocket_connection.js}
    # ⚡ JavaScript/Node.js

  ### isolated_screenshot_test.js {#agents-workspace-isolated_screenshot_test.js}
    # ⚡ JavaScript/Node.js

  ### iterative_validation_test.js {#agents-workspace-iterative_validation_test.js}
    # ⚡ JavaScript/Node.js

  ### milestone_1_console_capture_test.py {#agents-workspace-milestone_1_console_capture_test.py}
    # 🐍 Python

  ### milestone_3_console_reading_test.py {#agents-workspace-milestone_3_console_reading_test.py}
    # 🐍 Python

  ### README.md {#agents-workspace-readme.md}
    # 📖 Documentation

  ### ROADMAP.md {#agents-workspace-roadmap.md}
    # 📖 Documentation

  ### test_dual_connection.js {#agents-workspace-test_dual_connection.js}
    # ⚡ JavaScript/Node.js

  ### test_screenshot_with_debug.js {#agents-workspace-test_screenshot_with_debug.js}
    # ⚡ JavaScript/Node.js

  ### trace_websocket_screenshot.js {#agents-workspace-trace_websocket_screenshot.js}
    # ⚡ JavaScript/Node.js

  ### ui_debug_bootloader.py {#agents-workspace-ui_debug_bootloader.py}
    # 🐍 Python

  ### validate_claude_debug_capabilities.js {#agents-workspace-validate_claude_debug_capabilities.js}
    # ⚡ JavaScript/Node.js

    📁 **agents/workspace/docs/**
    ### CONTINUUM_MODEM_PROTOCOL_ROADMAP.md {#agents-workspace-docs-continuum_modem_protocol_roadmap.md}
      # 📖 Documentation

    📁 **agents/workspace/ui-debugging/**
    ### capture_full_ui_screenshot.py {#agents-workspace-ui-debugging-capture_full_ui_screenshot.py}
      # 🐍 Python

    ### check_js_console_errors.py {#agents-workspace-ui-debugging-check_js_console_errors.py}
      # 🐍 Python

    ### debug_component_loading.py {#agents-workspace-ui-debugging-debug_component_loading.py}
      # 🐍 Python

    ### fix_browser_tab_management.py {#agents-workspace-ui-debugging-fix_browser_tab_management.py}
      # 🐍 Python

    ### force_refresh_and_check.py {#agents-workspace-ui-debugging-force_refresh_and_check.py}
      # 🐍 Python

    ### force_server_cache_clear.py {#agents-workspace-ui-debugging-force_server_cache_clear.py}
      # 🐍 Python

    ### investigate_duplicate_agents_section.py {#agents-workspace-ui-debugging-investigate_duplicate_agents_section.py}
      # 🐍 Python

    ### investigate_duplicate_tabs.py {#agents-workspace-ui-debugging-investigate_duplicate_tabs.py}
      # 🐍 Python

    ### README.md {#agents-workspace-ui-debugging-readme.md}
      # 📖 Documentation

    ### sidebar_screenshot_workflow.py {#agents-workspace-ui-debugging-sidebar_screenshot_workflow.py}
      # 🐍 Python

    ### test_applescript_tab_detection.py {#agents-workspace-ui-debugging-test_applescript_tab_detection.py}
      # 🐍 Python

    ### test_manual_script_injection.py {#agents-workspace-ui-debugging-test_manual_script_injection.py}
      # 🐍 Python

    ### test_server_html_generation.py {#agents-workspace-ui-debugging-test_server_html_generation.py}
      # 🐍 Python

    ### verify_version_sync.py {#agents-workspace-ui-debugging-verify_version_sync.py}
      # 🐍 Python


📁 **archive/**
  📁 **archive/docs/**
  ### AI-INTELLIGENCE-VERIFIED.md {#archive-docs-ai-intelligence-verified.md}
    # 📦 Archived documentation

  ### CHANGELOG.md {#archive-docs-changelog.md}
    # 📦 Archived documentation

  ### CONTRIBUTING.md {#archive-docs-contributing.md}
    # 📦 Archived documentation

  ### LERNA_UPDATE.md {#archive-docs-lerna_update.md}
    # 📦 Archived documentation

  ### PR_CI_DESCRIPTION.md {#archive-docs-pr_ci_description.md}
    # 📦 Archived documentation

  ### PR_DESCRIPTION.md {#archive-docs-pr_description.md}
    # 📦 Archived documentation

  ### README-AI-HEALING.md {#archive-docs-readme-ai-healing.md}
    # 📦 Archived documentation

  ### RELEASING.md {#archive-docs-releasing.md}
    # 📦 Archived documentation

  ### ROADMAP.md {#archive-docs-roadmap.md}
    # 📖 Documentation

  ### SYSTEM_ARCHITECTURE.md {#archive-docs-system_architecture.md}
    # 📦 Archived documentation

    📁 **archive/docs/docs/**
    ### ai_assistant_config_tool.md {#archive-docs-docs-ai_assistant_config_tool.md}
      # 📦 Archived documentation

      📁 **archive/docs/docs/architecture/**
      ### implementation-specs.md {#archive-docs-docs-architecture-implementation-specs.md}
        # 📦 Archived documentation

      📁 **archive/docs/docs/design/**
      ### human-in-the-loop.md {#archive-docs-docs-design-human-in-the-loop.md}
        # 📦 Archived documentation

  📁 **archive/legacy-tests/**
  📁 **archive/old-experiments/**
  ### advanced-ai-system.cjs {#archive-old-experiments-advanced-ai-system.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### ai-process.cjs {#archive-old-experiments-ai-process.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### claude-auto-wrapper.cjs {#archive-old-experiments-claude-auto-wrapper.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### claude-cognition-test.cjs {#archive-old-experiments-claude-cognition-test.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### claude-direct.cjs {#archive-old-experiments-claude-direct.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### claude-qa-test.cjs {#archive-old-experiments-claude-qa-test.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### continuum-launcher.cjs {#archive-old-experiments-continuum-launcher.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### continuum.cjs {#archive-old-experiments-continuum.cjs}
    # ⚡ JavaScript/Node.js

  ### dynamic-ai-system.cjs {#archive-old-experiments-dynamic-ai-system.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### enhanced-ai-dev.cjs {#archive-old-experiments-enhanced-ai-dev.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### final-ai-system.cjs {#archive-old-experiments-final-ai-system.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### focused-ai-system.cjs {#archive-old-experiments-focused-ai-system.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### git-capable-ai.cjs {#archive-old-experiments-git-capable-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### github-ai-integration.cjs {#archive-old-experiments-github-ai-integration.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### guardian-ai-fixed.cjs {#archive-old-experiments-guardian-ai-fixed.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### guardian-ai.cjs {#archive-old-experiments-guardian-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### guardian-continuum.cjs {#archive-old-experiments-guardian-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### interactive-continuum.cjs {#archive-old-experiments-interactive-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### launch-continuum.cjs {#archive-old-experiments-launch-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### minimal-claude.cjs {#archive-old-experiments-minimal-claude.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### monitored-ai.cjs {#archive-old-experiments-monitored-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### multi-continuum.cjs {#archive-old-experiments-multi-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### nasa-grade-ai-dev.cjs {#archive-old-experiments-nasa-grade-ai-dev.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### organized-ai-process.cjs {#archive-old-experiments-organized-ai-process.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### organized-ai-system.cjs {#archive-old-experiments-organized-ai-system.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### pr-fixing-ai.cjs {#archive-old-experiments-pr-fixing-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### real-ai-interface.cjs {#archive-old-experiments-real-ai-interface.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### real-claude-connector.cjs {#archive-old-experiments-real-claude-connector.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### real-claude-pool.cjs {#archive-old-experiments-real-claude-pool.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### real-claude-tmux.cjs {#archive-old-experiments-real-claude-tmux.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### real-continuum.cjs {#archive-old-experiments-real-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### real-pool-manager.cjs {#archive-old-experiments-real-pool-manager.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### real-working-ai.cjs {#archive-old-experiments-real-working-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### realistic-continuum.cjs {#archive-old-experiments-realistic-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### resilient-ai-dev.cjs {#archive-old-experiments-resilient-ai-dev.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### self-healing-ai.cjs {#archive-old-experiments-self-healing-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### self-modifying-ai.cjs {#archive-old-experiments-self-modifying-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### self-modifying-continuum.cjs {#archive-old-experiments-self-modifying-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### simple-ai.cjs {#archive-old-experiments-simple-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### simple-claude-worker.cjs {#archive-old-experiments-simple-claude-worker.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### simple-test-ai.cjs {#archive-old-experiments-simple-test-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### simple-test.cjs {#archive-old-experiments-simple-test.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### smart-ai-coordinator.cjs {#archive-old-experiments-smart-ai-coordinator.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### talk-to-ai.cjs {#archive-old-experiments-talk-to-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### verified-ai-process.cjs {#archive-old-experiments-verified-ai-process.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### verified-ai-system.cjs {#archive-old-experiments-verified-ai-system.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### working-ai-system.cjs {#archive-old-experiments-working-ai-system.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### working-ai.cjs {#archive-old-experiments-working-ai.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### working-continuum.cjs {#archive-old-experiments-working-continuum.cjs}
    # 🗄️ Archived JavaScript (filed away)

  ### working-pool.cjs {#archive-old-experiments-working-pool.cjs}
    # 🗄️ Archived JavaScript (filed away)


📁 **archived/**
  📁 **archived/python-client/**
    📁 **archived/python-client/check/**
    ### check_browser_api.py {#archived-python-client-check-check_browser_api.py}
      # 🐍 Python

    ### check_browser_cache.py {#archived-python-client-check-check_browser_cache.py}
      # 🐍 Python

    ### check_console_errors.py {#archived-python-client-check-check_console_errors.py}
      # 🐍 Python

    ### check_js_syntax_errors.py {#archived-python-client-check-check_js_syntax_errors.py}
      # 🐍 Python

    📁 **archived/python-client/debug/**
    ### debug_continuum_api_loading.py {#archived-python-client-debug-debug_continuum_api_loading.py}
      # 🐍 Python

    ### debug_createpattern_error.py {#archived-python-client-debug-debug_createpattern_error.py}
      # 🐍 Python

    ### debug_createpattern_whole_screen.py {#archived-python-client-debug-debug_createpattern_whole_screen.py}
      # 🐍 Python

    ### debug_initialization_call.py {#archived-python-client-debug-debug_initialization_call.py}
      # 🐍 Python

    ### debug_server_processing.py {#archived-python-client-debug-debug_server_processing.py}
      # 🐍 Python

    ### debug_validation.py {#archived-python-client-debug-debug_validation.py}
      # 🐍 Python

    ### debug_with_scale.py {#archived-python-client-debug-debug_with_scale.py}
      # 🐍 Python

    📁 **archived/python-client/examples/**
    📁 **archived/python-client/fix/**
    ### fix_project_registration.py {#archived-python-client-fix-fix_project_registration.py}
      # 🐍 Python

    ### fixed_console_reader.py {#archived-python-client-fix-fixed_console_reader.py}
      # 🐍 Python

    📁 **archived/python-client/monitor/**
    ### monitor_screenshot_errors.py {#archived-python-client-monitor-monitor_screenshot_errors.py}
      # 🐍 Python

    ### realtime_monitor.py {#archived-python-client-monitor-realtime_monitor.py}
      # 🐍 Python

    📁 **archived/python-client/temp-files/**
    ### continuum-debug.log {#archived-python-client-temp-files-continuum-debug.log}
      # 🗑️ Log file (should be gitignored!)

    ### pyvenv.cfg {#archived-python-client-temp-files-pyvenv.cfg}
      # 🗂️ Archived file (shelved)

      📁 **archived/python-client/temp-files/bin/**
      ### activate {#archived-python-client-temp-files-bin-activate}
        # 🔧 Shell Script

      ### activate.csh {#archived-python-client-temp-files-bin-activate.csh}
        # 🗂️ Archived file (shelved)

      ### activate.fish {#archived-python-client-temp-files-bin-activate.fish}
        # 🗂️ Archived file (shelved)

      ### Activate.ps1 {#archived-python-client-temp-files-bin-activate.ps1}
        # 🗂️ Archived file (shelved)

      ### pip {#archived-python-client-temp-files-bin-pip}
        # 🗂️ Archived file (shelved)

      ### pip3 {#archived-python-client-temp-files-bin-pip3}
        # 🗂️ Archived file (shelved)

      ### pip3.9 {#archived-python-client-temp-files-bin-pip3.9}
        # 🗂️ Archived file (shelved)

      ### websockets {#archived-python-client-temp-files-bin-websockets}
        # 🗂️ Archived file (shelved)

      📁 **archived/python-client/temp-files/include/**
      📁 **archived/python-client/temp-files/lib/**
        📁 **archived/python-client/temp-files/lib/python3.9/**
          📁 **archived/python-client/temp-files/lib/python3.9/site-packages/**
          ### continuum-client.egg-link {#archived-python-client-temp-files-lib-python3.9-site-packages-continuum-client.egg-link}
            # 🗂️ Archived file (shelved)

          ### distutils-precedence.pth {#archived-python-client-temp-files-lib-python3.9-site-packages-distutils-precedence.pth}
            # 🗂️ Archived file (shelved)

          ### easy-install.pth {#archived-python-client-temp-files-lib-python3.9-site-packages-easy-install.pth}
            # 🗂️ Archived file (shelved)

            📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/**
            ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-__init__.py}
              # 🐍 Python

            ### __main__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-__main__.py}
              # 🐍 Python

            ### py.typed {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-py.typed}
              # 🗂️ Archived file (shelved)

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-__init__.py}
                # 🐍 Python

              ### build_env.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-build_env.py}
                # 🐍 Python

              ### cache.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cache.py}
                # 🐍 Python

              ### configuration.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-configuration.py}
                # 🐍 Python

              ### exceptions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-exceptions.py}
                # 🐍 Python

              ### main.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-main.py}
                # 🐍 Python

              ### pyproject.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-pyproject.py}
                # 🐍 Python

              ### self_outdated_check.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-self_outdated_check.py}
                # 🐍 Python

              ### wheel_builder.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-wheel_builder.py}
                # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/cli/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-__init__.py}
                  # 🐍 Python

                ### autocompletion.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-autocompletion.py}
                  # 🐍 Python

                ### base_command.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-base_command.py}
                  # 🐍 Python

                ### cmdoptions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-cmdoptions.py}
                  # 🐍 Python

                ### command_context.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-command_context.py}
                  # 🐍 Python

                ### main_parser.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-main_parser.py}
                  # 🐍 Python

                ### main.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-main.py}
                  # 🐍 Python

                ### parser.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-parser.py}
                  # 🐍 Python

                ### progress_bars.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-progress_bars.py}
                  # 🐍 Python

                ### req_command.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-req_command.py}
                  # 🐍 Python

                ### spinners.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-spinners.py}
                  # 🐍 Python

                ### status_codes.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-cli-status_codes.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/commands/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-__init__.py}
                  # 🐍 Python

                ### cache.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-cache.py}
                  # 🐍 Python

                ### check.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-check.py}
                  # 🐍 Python

                ### completion.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-completion.py}
                  # 🐍 Python

                ### configuration.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-configuration.py}
                  # 🐍 Python

                ### debug.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-debug.py}
                  # 🐍 Python

                ### download.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-download.py}
                  # 🐍 Python

                ### freeze.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-freeze.py}
                  # 🐍 Python

                ### hash.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-hash.py}
                  # 🐍 Python

                ### help.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-help.py}
                  # 🐍 Python

                ### index.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-index.py}
                  # 🐍 Python

                ### install.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-install.py}
                  # 🐍 Python

                ### list.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-list.py}
                  # 🐍 Python

                ### search.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-search.py}
                  # 🐍 Python

                ### show.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-show.py}
                  # 🐍 Python

                ### uninstall.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-uninstall.py}
                  # 🐍 Python

                ### wheel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-commands-wheel.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/index/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-index-__init__.py}
                  # 🐍 Python

                ### collector.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-index-collector.py}
                  # 🐍 Python

                ### package_finder.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-index-package_finder.py}
                  # 🐍 Python

                ### sources.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-index-sources.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/locations/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-locations-__init__.py}
                  # 🐍 Python

                ### _distutils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-locations-_distutils.py}
                  # 🐍 Python

                ### _sysconfig.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-locations-_sysconfig.py}
                  # 🐍 Python

                ### base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-locations-base.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/metadata/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-metadata-__init__.py}
                  # 🐍 Python

                ### base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-metadata-base.py}
                  # 🐍 Python

                ### pkg_resources.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-metadata-pkg_resources.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/models/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-__init__.py}
                  # 🐍 Python

                ### candidate.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-candidate.py}
                  # 🐍 Python

                ### direct_url.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-direct_url.py}
                  # 🐍 Python

                ### format_control.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-format_control.py}
                  # 🐍 Python

                ### index.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-index.py}
                  # 🐍 Python

                ### link.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-link.py}
                  # 🐍 Python

                ### scheme.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-scheme.py}
                  # 🐍 Python

                ### search_scope.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-search_scope.py}
                  # 🐍 Python

                ### selection_prefs.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-selection_prefs.py}
                  # 🐍 Python

                ### target_python.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-target_python.py}
                  # 🐍 Python

                ### wheel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-models-wheel.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/network/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-__init__.py}
                  # 🐍 Python

                ### auth.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-auth.py}
                  # 🐍 Python

                ### cache.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-cache.py}
                  # 🐍 Python

                ### download.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-download.py}
                  # 🐍 Python

                ### lazy_wheel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-lazy_wheel.py}
                  # 🐍 Python

                ### session.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-session.py}
                  # 🐍 Python

                ### utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-utils.py}
                  # 🐍 Python

                ### xmlrpc.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-network-xmlrpc.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/operations/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-__init__.py}
                  # 🐍 Python

                ### check.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-check.py}
                  # 🐍 Python

                ### freeze.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-freeze.py}
                  # 🐍 Python

                ### prepare.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-prepare.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/operations/install/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-install-__init__.py}
                    # 🐍 Python

                  ### editable_legacy.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-install-editable_legacy.py}
                    # 🐍 Python

                  ### legacy.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-install-legacy.py}
                    # 🐍 Python

                  ### wheel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-operations-install-wheel.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/req/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-req-__init__.py}
                  # 🐍 Python

                ### constructors.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-req-constructors.py}
                  # 🐍 Python

                ### req_file.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-req-req_file.py}
                  # 🐍 Python

                ### req_install.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-req-req_install.py}
                  # 🐍 Python

                ### req_set.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-req-req_set.py}
                  # 🐍 Python

                ### req_tracker.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-req-req_tracker.py}
                  # 🐍 Python

                ### req_uninstall.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-req-req_uninstall.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/resolution/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-__init__.py}
                  # 🐍 Python

                ### base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-base.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/resolution/legacy/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-legacy-__init__.py}
                    # 🐍 Python

                  ### resolver.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-legacy-resolver.py}
                    # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/resolution/resolvelib/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-__init__.py}
                    # 🐍 Python

                  ### base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-base.py}
                    # 🐍 Python

                  ### candidates.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-candidates.py}
                    # 🐍 Python

                  ### factory.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-factory.py}
                    # 🐍 Python

                  ### found_candidates.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-found_candidates.py}
                    # 🐍 Python

                  ### provider.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-provider.py}
                    # 🐍 Python

                  ### reporter.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-reporter.py}
                    # 🐍 Python

                  ### requirements.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-requirements.py}
                    # 🐍 Python

                  ### resolver.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-resolution-resolvelib-resolver.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/utils/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-__init__.py}
                  # 🐍 Python

                ### _log.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-_log.py}
                  # 🐍 Python

                ### appdirs.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-appdirs.py}
                  # 🐍 Python

                ### compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-compat.py}
                  # 🐍 Python

                ### compatibility_tags.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-compatibility_tags.py}
                  # 🐍 Python

                ### datetime.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-datetime.py}
                  # 🐍 Python

                ### deprecation.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-deprecation.py}
                  # 🐍 Python

                ### direct_url_helpers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-direct_url_helpers.py}
                  # 🐍 Python

                ### distutils_args.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-distutils_args.py}
                  # 🐍 Python

                ### encoding.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-encoding.py}
                  # 🐍 Python

                ### entrypoints.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-entrypoints.py}
                  # 🐍 Python

                ### filesystem.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-filesystem.py}
                  # 🐍 Python

                ### filetypes.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-filetypes.py}
                  # 🐍 Python

                ### glibc.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-glibc.py}
                  # 🐍 Python

                ### hashes.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-hashes.py}
                  # 🐍 Python

                ### inject_securetransport.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-inject_securetransport.py}
                  # 🐍 Python

                ### logging.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-logging.py}
                  # 🐍 Python

                ### misc.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-misc.py}
                  # 🐍 Python

                ### models.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-models.py}
                  # 🐍 Python

                ### packaging.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-packaging.py}
                  # 🐍 Python

                ### parallel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-parallel.py}
                  # 🐍 Python

                ### pkg_resources.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-pkg_resources.py}
                  # 🐍 Python

                ### setuptools_build.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-setuptools_build.py}
                  # 🐍 Python

                ### subprocess.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-subprocess.py}
                  # 🐍 Python

                ### temp_dir.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-temp_dir.py}
                  # 🐍 Python

                ### unpacking.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-unpacking.py}
                  # 🐍 Python

                ### urls.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-urls.py}
                  # 🐍 Python

                ### virtualenv.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-virtualenv.py}
                  # 🐍 Python

                ### wheel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-utils-wheel.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_internal/vcs/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-vcs-__init__.py}
                  # 🐍 Python

                ### bazaar.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-vcs-bazaar.py}
                  # 🐍 Python

                ### git.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-vcs-git.py}
                  # 🐍 Python

                ### mercurial.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-vcs-mercurial.py}
                  # 🐍 Python

                ### subversion.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-vcs-subversion.py}
                  # 🐍 Python

                ### versioncontrol.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_internal-vcs-versioncontrol.py}
                  # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-__init__.py}
                # 🐍 Python

              ### appdirs.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-appdirs.py}
                # 🐍 Python

              ### distro.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-distro.py}
                # 🐍 Python

              ### pyparsing.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pyparsing.py}
                # 🐍 Python

              ### six.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-six.py}
                # 🐍 Python

              ### vendor.txt {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-vendor.txt}
                # 🗂️ Archived file (shelved)

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/cachecontrol/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-__init__.py}
                  # 🐍 Python

                ### _cmd.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-_cmd.py}
                  # 🐍 Python

                ### adapter.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-adapter.py}
                  # 🐍 Python

                ### cache.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-cache.py}
                  # 🐍 Python

                ### compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-compat.py}
                  # 🐍 Python

                ### controller.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-controller.py}
                  # 🐍 Python

                ### filewrapper.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-filewrapper.py}
                  # 🐍 Python

                ### heuristics.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-heuristics.py}
                  # 🐍 Python

                ### serialize.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-serialize.py}
                  # 🐍 Python

                ### wrapper.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-wrapper.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/cachecontrol/caches/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-caches-__init__.py}
                    # 🐍 Python

                  ### file_cache.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-caches-file_cache.py}
                    # 🐍 Python

                  ### redis_cache.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-cachecontrol-caches-redis_cache.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/certifi/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-certifi-__init__.py}
                  # 🐍 Python

                ### __main__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-certifi-__main__.py}
                  # 🐍 Python

                ### cacert.pem {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-certifi-cacert.pem}
                  # 🗂️ Archived file (shelved)

                ### core.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-certifi-core.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/chardet/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-__init__.py}
                  # 🐍 Python

                ### big5freq.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-big5freq.py}
                  # 🐍 Python

                ### big5prober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-big5prober.py}
                  # 🐍 Python

                ### chardistribution.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-chardistribution.py}
                  # 🐍 Python

                ### charsetgroupprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-charsetgroupprober.py}
                  # 🐍 Python

                ### charsetprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-charsetprober.py}
                  # 🐍 Python

                ### codingstatemachine.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-codingstatemachine.py}
                  # 🐍 Python

                ### compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-compat.py}
                  # 🐍 Python

                ### cp949prober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-cp949prober.py}
                  # 🐍 Python

                ### enums.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-enums.py}
                  # 🐍 Python

                ### escprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-escprober.py}
                  # 🐍 Python

                ### escsm.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-escsm.py}
                  # 🐍 Python

                ### eucjpprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-eucjpprober.py}
                  # 🐍 Python

                ### euckrfreq.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-euckrfreq.py}
                  # 🐍 Python

                ### euckrprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-euckrprober.py}
                  # 🐍 Python

                ### euctwfreq.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-euctwfreq.py}
                  # 🐍 Python

                ### euctwprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-euctwprober.py}
                  # 🐍 Python

                ### gb2312freq.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-gb2312freq.py}
                  # 🐍 Python

                ### gb2312prober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-gb2312prober.py}
                  # 🐍 Python

                ### hebrewprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-hebrewprober.py}
                  # 🐍 Python

                ### jisfreq.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-jisfreq.py}
                  # 🐍 Python

                ### jpcntx.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-jpcntx.py}
                  # 🐍 Python

                ### langbulgarianmodel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-langbulgarianmodel.py}
                  # 🐍 Python

                ### langgreekmodel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-langgreekmodel.py}
                  # 🐍 Python

                ### langhebrewmodel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-langhebrewmodel.py}
                  # 🐍 Python

                ### langhungarianmodel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-langhungarianmodel.py}
                  # 🐍 Python

                ### langrussianmodel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-langrussianmodel.py}
                  # 🐍 Python

                ### langthaimodel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-langthaimodel.py}
                  # 🐍 Python

                ### langturkishmodel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-langturkishmodel.py}
                  # 🐍 Python

                ### latin1prober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-latin1prober.py}
                  # 🐍 Python

                ### mbcharsetprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-mbcharsetprober.py}
                  # 🐍 Python

                ### mbcsgroupprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-mbcsgroupprober.py}
                  # 🐍 Python

                ### mbcssm.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-mbcssm.py}
                  # 🐍 Python

                ### sbcharsetprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-sbcharsetprober.py}
                  # 🐍 Python

                ### sbcsgroupprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-sbcsgroupprober.py}
                  # 🐍 Python

                ### sjisprober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-sjisprober.py}
                  # 🐍 Python

                ### universaldetector.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-universaldetector.py}
                  # 🐍 Python

                ### utf8prober.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-utf8prober.py}
                  # 🐍 Python

                ### version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-version.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/chardet/cli/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-cli-__init__.py}
                    # 🐍 Python

                  ### chardetect.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-cli-chardetect.py}
                    # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/chardet/metadata/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-metadata-__init__.py}
                    # 🐍 Python

                  ### languages.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-chardet-metadata-languages.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/colorama/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-colorama-__init__.py}
                  # 🐍 Python

                ### ansi.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-colorama-ansi.py}
                  # 🐍 Python

                ### ansitowin32.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-colorama-ansitowin32.py}
                  # 🐍 Python

                ### initialise.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-colorama-initialise.py}
                  # 🐍 Python

                ### win32.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-colorama-win32.py}
                  # 🐍 Python

                ### winterm.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-colorama-winterm.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/html5lib/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-__init__.py}
                  # 🐍 Python

                ### _ihatexml.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-_ihatexml.py}
                  # 🐍 Python

                ### _inputstream.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-_inputstream.py}
                  # 🐍 Python

                ### _tokenizer.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-_tokenizer.py}
                  # 🐍 Python

                ### _utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-_utils.py}
                  # 🐍 Python

                ### constants.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-constants.py}
                  # 🐍 Python

                ### html5parser.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-html5parser.py}
                  # 🐍 Python

                ### serializer.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-serializer.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/html5lib/_trie/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-_trie-__init__.py}
                    # 🐍 Python

                  ### _base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-_trie-_base.py}
                    # 🐍 Python

                  ### py.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-_trie-py.py}
                    # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/html5lib/filters/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-__init__.py}
                    # 🐍 Python

                  ### alphabeticalattributes.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-alphabeticalattributes.py}
                    # 🐍 Python

                  ### base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-base.py}
                    # 🐍 Python

                  ### inject_meta_charset.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-inject_meta_charset.py}
                    # 🐍 Python

                  ### lint.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-lint.py}
                    # 🐍 Python

                  ### optionaltags.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-optionaltags.py}
                    # 🐍 Python

                  ### sanitizer.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-sanitizer.py}
                    # 🐍 Python

                  ### whitespace.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-filters-whitespace.py}
                    # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/html5lib/treeadapters/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treeadapters-__init__.py}
                    # 🐍 Python

                  ### genshi.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treeadapters-genshi.py}
                    # 🐍 Python

                  ### sax.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treeadapters-sax.py}
                    # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/html5lib/treewalkers/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treewalkers-__init__.py}
                    # 🐍 Python

                  ### base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treewalkers-base.py}
                    # 🐍 Python

                  ### dom.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treewalkers-dom.py}
                    # 🐍 Python

                  ### etree_lxml.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treewalkers-etree_lxml.py}
                    # 🐍 Python

                  ### etree.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treewalkers-etree.py}
                    # 🐍 Python

                  ### genshi.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-html5lib-treewalkers-genshi.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/idna/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-__init__.py}
                  # 🐍 Python

                ### codec.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-codec.py}
                  # 🐍 Python

                ### compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-compat.py}
                  # 🐍 Python

                ### core.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-core.py}
                  # 🐍 Python

                ### idnadata.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-idnadata.py}
                  # 🐍 Python

                ### intranges.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-intranges.py}
                  # 🐍 Python

                ### package_data.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-package_data.py}
                  # 🐍 Python

                ### uts46data.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-idna-uts46data.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/msgpack/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-msgpack-__init__.py}
                  # 🐍 Python

                ### _version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-msgpack-_version.py}
                  # 🐍 Python

                ### exceptions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-msgpack-exceptions.py}
                  # 🐍 Python

                ### ext.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-msgpack-ext.py}
                  # 🐍 Python

                ### fallback.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-msgpack-fallback.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/packaging/**
                ### __about__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-__about__.py}
                  # 🐍 Python

                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-__init__.py}
                  # 🐍 Python

                ### _manylinux.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-_manylinux.py}
                  # 🐍 Python

                ### _musllinux.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-_musllinux.py}
                  # 🐍 Python

                ### _structures.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-_structures.py}
                  # 🐍 Python

                ### markers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-markers.py}
                  # 🐍 Python

                ### requirements.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-requirements.py}
                  # 🐍 Python

                ### specifiers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-specifiers.py}
                  # 🐍 Python

                ### tags.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-tags.py}
                  # 🐍 Python

                ### utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-utils.py}
                  # 🐍 Python

                ### version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-packaging-version.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/pep517/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-__init__.py}
                  # 🐍 Python

                ### build.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-build.py}
                  # 🐍 Python

                ### check.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-check.py}
                  # 🐍 Python

                ### colorlog.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-colorlog.py}
                  # 🐍 Python

                ### compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-compat.py}
                  # 🐍 Python

                ### dirtools.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-dirtools.py}
                  # 🐍 Python

                ### envbuild.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-envbuild.py}
                  # 🐍 Python

                ### meta.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-meta.py}
                  # 🐍 Python

                ### wrappers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-wrappers.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/pep517/in_process/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-in_process-__init__.py}
                    # 🐍 Python

                  ### _in_process.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pep517-in_process-_in_process.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/pkg_resources/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pkg_resources-__init__.py}
                  # 🐍 Python

                ### py31compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-pkg_resources-py31compat.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/progress/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-progress-__init__.py}
                  # 🐍 Python

                ### bar.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-progress-bar.py}
                  # 🐍 Python

                ### counter.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-progress-counter.py}
                  # 🐍 Python

                ### spinner.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-progress-spinner.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/requests/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-__init__.py}
                  # 🐍 Python

                ### __version__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-__version__.py}
                  # 🐍 Python

                ### _internal_utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-_internal_utils.py}
                  # 🐍 Python

                ### adapters.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-adapters.py}
                  # 🐍 Python

                ### api.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-api.py}
                  # 🐍 Python

                ### auth.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-auth.py}
                  # 🐍 Python

                ### certs.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-certs.py}
                  # 🐍 Python

                ### compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-compat.py}
                  # 🐍 Python

                ### cookies.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-cookies.py}
                  # 🐍 Python

                ### exceptions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-exceptions.py}
                  # 🐍 Python

                ### help.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-help.py}
                  # 🐍 Python

                ### hooks.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-hooks.py}
                  # 🐍 Python

                ### models.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-models.py}
                  # 🐍 Python

                ### packages.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-packages.py}
                  # 🐍 Python

                ### sessions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-sessions.py}
                  # 🐍 Python

                ### status_codes.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-status_codes.py}
                  # 🐍 Python

                ### structures.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-structures.py}
                  # 🐍 Python

                ### utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-requests-utils.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/resolvelib/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-resolvelib-__init__.py}
                  # 🐍 Python

                ### providers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-resolvelib-providers.py}
                  # 🐍 Python

                ### reporters.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-resolvelib-reporters.py}
                  # 🐍 Python

                ### resolvers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-resolvelib-resolvers.py}
                  # 🐍 Python

                ### structs.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-resolvelib-structs.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/resolvelib/compat/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-resolvelib-compat-__init__.py}
                    # 🐍 Python

                  ### collections_abc.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-resolvelib-compat-collections_abc.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/tenacity/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-__init__.py}
                  # 🐍 Python

                ### _asyncio.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-_asyncio.py}
                  # 🐍 Python

                ### _utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-_utils.py}
                  # 🐍 Python

                ### after.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-after.py}
                  # 🐍 Python

                ### before_sleep.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-before_sleep.py}
                  # 🐍 Python

                ### before.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-before.py}
                  # 🐍 Python

                ### nap.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-nap.py}
                  # 🐍 Python

                ### retry.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-retry.py}
                  # 🐍 Python

                ### stop.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-stop.py}
                  # 🐍 Python

                ### tornadoweb.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-tornadoweb.py}
                  # 🐍 Python

                ### wait.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tenacity-wait.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/tomli/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tomli-__init__.py}
                  # 🐍 Python

                ### _parser.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tomli-_parser.py}
                  # 🐍 Python

                ### _re.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-tomli-_re.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/urllib3/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-__init__.py}
                  # 🐍 Python

                ### _collections.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-_collections.py}
                  # 🐍 Python

                ### _version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-_version.py}
                  # 🐍 Python

                ### connection.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-connection.py}
                  # 🐍 Python

                ### connectionpool.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-connectionpool.py}
                  # 🐍 Python

                ### exceptions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-exceptions.py}
                  # 🐍 Python

                ### fields.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-fields.py}
                  # 🐍 Python

                ### filepost.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-filepost.py}
                  # 🐍 Python

                ### poolmanager.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-poolmanager.py}
                  # 🐍 Python

                ### request.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-request.py}
                  # 🐍 Python

                ### response.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-response.py}
                  # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/urllib3/contrib/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-__init__.py}
                    # 🐍 Python

                  ### _appengine_environ.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-_appengine_environ.py}
                    # 🐍 Python

                  ### appengine.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-appengine.py}
                    # 🐍 Python

                  ### ntlmpool.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-ntlmpool.py}
                    # 🐍 Python

                  ### pyopenssl.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-pyopenssl.py}
                    # 🐍 Python

                  ### securetransport.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-securetransport.py}
                    # 🐍 Python

                  ### socks.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-socks.py}
                    # 🐍 Python

                    📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/urllib3/contrib/_securetransport/**
                    ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-_securetransport-__init__.py}
                      # 🐍 Python

                    ### bindings.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-_securetransport-bindings.py}
                      # 🐍 Python

                    ### low_level.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-contrib-_securetransport-low_level.py}
                      # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/urllib3/packages/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-packages-__init__.py}
                    # 🐍 Python

                  ### six.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-packages-six.py}
                    # 🐍 Python

                    📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/urllib3/packages/backports/**
                    ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-packages-backports-__init__.py}
                      # 🐍 Python

                    ### makefile.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-packages-backports-makefile.py}
                      # 🐍 Python

                    📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/urllib3/packages/ssl_match_hostname/**
                    ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-packages-ssl_match_hostname-__init__.py}
                      # 🐍 Python

                    ### _implementation.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-packages-ssl_match_hostname-_implementation.py}
                      # 🐍 Python

                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/urllib3/util/**
                  ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-__init__.py}
                    # 🐍 Python

                  ### connection.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-connection.py}
                    # 🐍 Python

                  ### proxy.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-proxy.py}
                    # 🐍 Python

                  ### queue.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-queue.py}
                    # 🐍 Python

                  ### request.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-request.py}
                    # 🐍 Python

                  ### response.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-response.py}
                    # 🐍 Python

                  ### retry.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-retry.py}
                    # 🐍 Python

                  ### ssl_.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-ssl_.py}
                    # 🐍 Python

                  ### ssltransport.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-ssltransport.py}
                    # 🐍 Python

                  ### timeout.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-timeout.py}
                    # 🐍 Python

                  ### url.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-url.py}
                    # 🐍 Python

                  ### wait.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-urllib3-util-wait.py}
                    # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pip/_vendor/webencodings/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-webencodings-__init__.py}
                  # 🐍 Python

                ### labels.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-webencodings-labels.py}
                  # 🐍 Python

                ### mklabels.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-webencodings-mklabels.py}
                  # 🐍 Python

                ### tests.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-webencodings-tests.py}
                  # 🐍 Python

                ### x_user_defined.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pip-_vendor-webencodings-x_user_defined.py}
                  # 🐍 Python

            📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pkg_resources/**
            ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-__init__.py}
              # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pkg_resources/_vendor/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-__init__.py}
                # 🐍 Python

              ### appdirs.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-appdirs.py}
                # 🐍 Python

              ### pyparsing.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-pyparsing.py}
                # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pkg_resources/_vendor/packaging/**
                ### __about__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-__about__.py}
                  # 🐍 Python

                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-__init__.py}
                  # 🐍 Python

                ### _compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-_compat.py}
                  # 🐍 Python

                ### _structures.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-_structures.py}
                  # 🐍 Python

                ### _typing.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-_typing.py}
                  # 🐍 Python

                ### markers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-markers.py}
                  # 🐍 Python

                ### requirements.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-requirements.py}
                  # 🐍 Python

                ### specifiers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-specifiers.py}
                  # 🐍 Python

                ### tags.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-tags.py}
                  # 🐍 Python

                ### utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-utils.py}
                  # 🐍 Python

                ### version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-_vendor-packaging-version.py}
                  # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pkg_resources/extern/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-extern-__init__.py}
                # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pkg_resources/tests/**
                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pkg_resources/tests/data/**
                  📁 **archived/python-client/temp-files/lib/python3.9/site-packages/pkg_resources/tests/data/my-test-package-source/**
                  ### setup.py {#archived-python-client-temp-files-lib-python3.9-site-packages-pkg_resources-tests-data-my-test-package-source-setup.py}
                    # 🐍 Python

            📁 **archived/python-client/temp-files/lib/python3.9/site-packages/setuptools/**
            ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-__init__.py}
              # 🐍 Python

            ### _deprecation_warning.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_deprecation_warning.py}
              # 🐍 Python

            ### _imp.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_imp.py}
              # 🐍 Python

            ### archive_util.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-archive_util.py}
              # 🐍 Python

            ### build_meta.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-build_meta.py}
              # 🐍 Python

            ### cli-32.exe {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-cli-32.exe}
              # 🗂️ Archived file (shelved)

            ### cli-64.exe {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-cli-64.exe}
              # 🗂️ Archived file (shelved)

            ### cli.exe {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-cli.exe}
              # 🗂️ Archived file (shelved)

            ### config.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-config.py}
              # 🐍 Python

            ### dep_util.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-dep_util.py}
              # 🐍 Python

            ### depends.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-depends.py}
              # 🐍 Python

            ### dist.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-dist.py}
              # 🐍 Python

            ### errors.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-errors.py}
              # 🐍 Python

            ### extension.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-extension.py}
              # 🐍 Python

            ### glob.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-glob.py}
              # 🐍 Python

            ### gui-32.exe {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-gui-32.exe}
              # 🗂️ Archived file (shelved)

            ### gui-64.exe {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-gui-64.exe}
              # 🗂️ Archived file (shelved)

            ### gui.exe {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-gui.exe}
              # 🗂️ Archived file (shelved)

            ### installer.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-installer.py}
              # 🐍 Python

            ### launch.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-launch.py}
              # 🐍 Python

            ### monkey.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-monkey.py}
              # 🐍 Python

            ### msvc.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-msvc.py}
              # 🐍 Python

            ### namespaces.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-namespaces.py}
              # 🐍 Python

            ### package_index.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-package_index.py}
              # 🐍 Python

            ### py34compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-py34compat.py}
              # 🐍 Python

            ### sandbox.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-sandbox.py}
              # 🐍 Python

            ### script (dev).tmpl {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-script--dev-.tmpl}
              # 🗂️ Archived file (shelved)

            ### script.tmpl {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-script.tmpl}
              # 🗂️ Archived file (shelved)

            ### unicode_utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-unicode_utils.py}
              # 🐍 Python

            ### version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-version.py}
              # 🐍 Python

            ### wheel.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-wheel.py}
              # 🐍 Python

            ### windows_support.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-windows_support.py}
              # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/setuptools/_vendor/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-__init__.py}
                # 🐍 Python

              ### ordered_set.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-ordered_set.py}
                # 🐍 Python

              ### pyparsing.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-pyparsing.py}
                # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/setuptools/_vendor/more_itertools/**
                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-more_itertools-__init__.py}
                  # 🐍 Python

                ### more.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-more_itertools-more.py}
                  # 🐍 Python

                ### recipes.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-more_itertools-recipes.py}
                  # 🐍 Python

                📁 **archived/python-client/temp-files/lib/python3.9/site-packages/setuptools/_vendor/packaging/**
                ### __about__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-__about__.py}
                  # 🐍 Python

                ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-__init__.py}
                  # 🐍 Python

                ### _compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-_compat.py}
                  # 🐍 Python

                ### _structures.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-_structures.py}
                  # 🐍 Python

                ### _typing.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-_typing.py}
                  # 🐍 Python

                ### markers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-markers.py}
                  # 🐍 Python

                ### requirements.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-requirements.py}
                  # 🐍 Python

                ### specifiers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-specifiers.py}
                  # 🐍 Python

                ### tags.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-tags.py}
                  # 🐍 Python

                ### utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-utils.py}
                  # 🐍 Python

                ### version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-_vendor-packaging-version.py}
                  # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/setuptools/command/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-__init__.py}
                # 🐍 Python

              ### alias.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-alias.py}
                # 🐍 Python

              ### bdist_egg.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-bdist_egg.py}
                # 🐍 Python

              ### bdist_rpm.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-bdist_rpm.py}
                # 🐍 Python

              ### build_clib.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-build_clib.py}
                # 🐍 Python

              ### build_ext.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-build_ext.py}
                # 🐍 Python

              ### build_py.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-build_py.py}
                # 🐍 Python

              ### develop.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-develop.py}
                # 🐍 Python

              ### dist_info.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-dist_info.py}
                # 🐍 Python

              ### easy_install.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-easy_install.py}
                # 🐍 Python

              ### egg_info.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-egg_info.py}
                # 🐍 Python

              ### install_egg_info.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-install_egg_info.py}
                # 🐍 Python

              ### install_lib.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-install_lib.py}
                # 🐍 Python

              ### install_scripts.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-install_scripts.py}
                # 🐍 Python

              ### install.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-install.py}
                # 🐍 Python

              ### launcher manifest.xml {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-launcher-manifest.xml}
                # 🗂️ Archived file (shelved)

              ### py36compat.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-py36compat.py}
                # 🐍 Python

              ### register.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-register.py}
                # 🐍 Python

              ### rotate.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-rotate.py}
                # 🐍 Python

              ### saveopts.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-saveopts.py}
                # 🐍 Python

              ### sdist.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-sdist.py}
                # 🐍 Python

              ### setopt.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-setopt.py}
                # 🐍 Python

              ### test.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-test.py}
                # 🐍 Python

              ### upload_docs.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-upload_docs.py}
                # 🐍 Python

              ### upload.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-command-upload.py}
                # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/setuptools/extern/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-setuptools-extern-__init__.py}
                # 🐍 Python

            📁 **archived/python-client/temp-files/lib/python3.9/site-packages/websockets/**
            ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-__init__.py}
              # 🐍 Python

            ### __main__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-__main__.py}
              # 🐍 Python

            ### auth.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-auth.py}
              # 🐍 Python

            ### cli.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-cli.py}
              # 🐍 Python

            ### client.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-client.py}
              # 🐍 Python

            ### connection.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-connection.py}
              # 🐍 Python

            ### datastructures.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-datastructures.py}
              # 🐍 Python

            ### exceptions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-exceptions.py}
              # 🐍 Python

            ### frames.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-frames.py}
              # 🐍 Python

            ### headers.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-headers.py}
              # 🐍 Python

            ### http.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-http.py}
              # 🐍 Python

            ### http11.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-http11.py}
              # 🐍 Python

            ### imports.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-imports.py}
              # 🐍 Python

            ### protocol.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-protocol.py}
              # 🐍 Python

            ### py.typed {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-py.typed}
              # 🗂️ Archived file (shelved)

            ### server.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-server.py}
              # 🐍 Python

            ### speedups.c {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-speedups.c}
              # 🗂️ Archived file (shelved)

            ### speedups.cpython-39-darwin.so {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-speedups.cpython-39-darwin.so}
              # 🗂️ Archived file (shelved)

            ### speedups.pyi {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-speedups.pyi}
              # 🗂️ Archived file (shelved)

            ### streams.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-streams.py}
              # 🐍 Python

            ### typing.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-typing.py}
              # 🐍 Python

            ### uri.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-uri.py}
              # 🐍 Python

            ### utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-utils.py}
              # 🐍 Python

            ### version.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-version.py}
              # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/websockets/asyncio/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-__init__.py}
                # 🐍 Python

              ### async_timeout.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-async_timeout.py}
                # 🐍 Python

              ### client.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-client.py}
                # 🐍 Python

              ### compatibility.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-compatibility.py}
                # 🐍 Python

              ### connection.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-connection.py}
                # 🐍 Python

              ### messages.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-messages.py}
                # 🐍 Python

              ### router.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-router.py}
                # 🐍 Python

              ### server.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-asyncio-server.py}
                # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/websockets/extensions/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-extensions-__init__.py}
                # 🐍 Python

              ### base.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-extensions-base.py}
                # 🐍 Python

              ### permessage_deflate.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-extensions-permessage_deflate.py}
                # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/websockets/legacy/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-__init__.py}
                # 🐍 Python

              ### auth.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-auth.py}
                # 🐍 Python

              ### client.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-client.py}
                # 🐍 Python

              ### exceptions.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-exceptions.py}
                # 🐍 Python

              ### framing.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-framing.py}
                # 🐍 Python

              ### handshake.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-handshake.py}
                # 🐍 Python

              ### http.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-http.py}
                # 🐍 Python

              ### protocol.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-protocol.py}
                # 🐍 Python

              ### server.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-legacy-server.py}
                # 🐍 Python

              📁 **archived/python-client/temp-files/lib/python3.9/site-packages/websockets/sync/**
              ### __init__.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-sync-__init__.py}
                # 🐍 Python

              ### client.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-sync-client.py}
                # 🐍 Python

              ### connection.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-sync-connection.py}
                # 🐍 Python

              ### messages.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-sync-messages.py}
                # 🐍 Python

              ### router.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-sync-router.py}
                # 🐍 Python

              ### server.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-sync-server.py}
                # 🐍 Python

              ### utils.py {#archived-python-client-temp-files-lib-python3.9-site-packages-websockets-sync-utils.py}
                # 🐍 Python

    📁 **archived/python-client/test-scripts/**
    ### full_page_debug.py {#archived-python-client-test-scripts-full_page_debug.py}
      # 🐍 Python

    ### run_console_check.py {#archived-python-client-test-scripts-run_console_check.py}
      # 🐍 Python

    ### server_file_save_hook.js {#archived-python-client-test-scripts-server_file_save_hook.js}
      # 🗄️ Archived JavaScript (filed away)

      📁 **archived/python-client/test-scripts/test_screenshots/**
      ### bytes_mode_version.png {#archived-python-client-test-scripts-test_screenshots-bytes_mode_version.png}
        # 🗂️ Archived file (shelved)

  📁 **archived/root-level/**
    📁 **archived/root-level/temp-files/**
    ### continuon.markdown {#archived-root-level-temp-files-continuon.markdown}
      # 🗂️ Archived file (shelved)

    ### continuum_restart.log {#archived-root-level-temp-files-continuum_restart.log}
      # 🗑️ Log file (should be gitignored!)

    ### continuum-core.log {#archived-root-level-temp-files-continuum-core.log}
      # 🗑️ Log file (should be gitignored!)

    ### continuum-fixed.log {#archived-root-level-temp-files-continuum-fixed.log}
      # 🗑️ Log file (should be gitignored!)

    ### continuum.cjs.OLD {#archived-root-level-temp-files-continuum.cjs.old}
      # 🗂️ Archived file (shelved)

    ### continuum.log {#archived-root-level-temp-files-continuum.log}
      # 📄 File

    ### daemon-crash.log {#archived-root-level-temp-files-daemon-crash.log}
      # 🗑️ Log file (should be gitignored!)

    ### daemon-debug.log {#archived-root-level-temp-files-daemon-debug.log}
      # 🗑️ Log file (should be gitignored!)

    ### debug-academy-ui.html {#archived-root-level-temp-files-debug-academy-ui.html}
      # 🗃️ Archived HTML (stored away)

    ### debug-ui.html {#archived-root-level-temp-files-debug-ui.html}
      # 🗃️ Archived HTML (stored away)

    ### FluentAPI.cjs.bak {#archived-root-level-temp-files-fluentapi.cjs.bak}
      # 🗂️ Archived file (shelved)

    ### latest-daemon-attempt.log {#archived-root-level-temp-files-latest-daemon-attempt.log}
      # 🗑️ Log file (should be gitignored!)

    ### MoveCommand.cjs.bak2 {#archived-root-level-temp-files-movecommand.cjs.bak2}
      # 🗂️ Archived file (shelved)

    ### MoveCommand.cjs.bak3 {#archived-root-level-temp-files-movecommand.cjs.bak3}
      # 🗂️ Archived file (shelved)

    ### server.log {#archived-root-level-temp-files-server.log}
      # 📄 File

    ### simple-daemon.cjs {#archived-root-level-temp-files-simple-daemon.cjs}
      # 🗄️ Archived JavaScript (filed away)

      📁 **archived/root-level/temp-files/test-run/**
      📁 **archived/root-level/temp-files/untitled folder/**
    📁 **archived/root-level/test-files/**
    ### browser_client_validation_simple.py {#archived-root-level-test-files-browser_client_validation_simple.py}
      # 🐍 Python

    ### capture_real_screenshot.py {#archived-root-level-test-files-capture_real_screenshot.py}
      # 🐍 Python

    ### chat-with-user.cjs {#archived-root-level-test-files-chat-with-user.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### check_screenshot_logs.py {#archived-root-level-test-files-check_screenshot_logs.py}
      # 🐍 Python

    ### check-imports.cjs {#archived-root-level-test-files-check-imports.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### claude-code-agent.cjs {#archived-root-level-test-files-claude-code-agent.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### complete_browser_client_validation.py {#archived-root-level-test-files-complete_browser_client_validation.py}
      # 🐍 Python

    ### connect_both_clients_to_bus.py {#archived-root-level-test-files-connect_both_clients_to_bus.py}
      # 🐍 Python

    ### connection_aware_validator.py {#archived-root-level-test-files-connection_aware_validator.py}
      # 🐍 Python

    ### continuum-web-browser-test.cjs {#archived-root-level-test-files-continuum-web-browser-test.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### cursor-movement-demo.cjs {#archived-root-level-test-files-cursor-movement-demo.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### debug_canvas_elements.py {#archived-root-level-test-files-debug_canvas_elements.py}
      # 🐍 Python

    ### debug_m6_console.py {#archived-root-level-test-files-debug_m6_console.py}
      # 🐍 Python

    ### debug_screenshot_console.py {#archived-root-level-test-files-debug_screenshot_console.py}
      # 🐍 Python

    ### debug_with_logs.py {#archived-root-level-test-files-debug_with_logs.py}
      # 🐍 Python

    ### debug-drawer.cjs {#archived-root-level-test-files-debug-drawer.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### demo-claude-agent.cjs {#archived-root-level-test-files-demo-claude-agent.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### demo-visual-gaming.cjs {#archived-root-level-test-files-demo-visual-gaming.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### dev-shop-coordinator.cjs {#archived-root-level-test-files-dev-shop-coordinator.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### direct-test.cjs {#archived-root-level-test-files-direct-test.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### final-message.cjs {#archived-root-level-test-files-final-message.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### fix_browser_connection_and_m6.py {#archived-root-level-test-files-fix_browser_connection_and_m6.py}
      # 🐍 Python

    ### fix_browser_ws.py {#archived-root-level-test-files-fix_browser_ws.py}
      # 🐍 Python

    ### intelligent-pr-monitor.cjs {#archived-root-level-test-files-intelligent-pr-monitor.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### jest.config.test.js {#archived-root-level-test-files-jest.config.test.js}
      # 🗄️ Archived JavaScript (filed away)

    ### live-cyberpunk-dev.cjs {#archived-root-level-test-files-live-cyberpunk-dev.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### monitor-pr-63.cjs {#archived-root-level-test-files-monitor-pr-63.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### monitored-spawn.cjs {#archived-root-level-test-files-monitored-spawn.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### pr-monitor-bot.cjs {#archived-root-level-test-files-pr-monitor-bot.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### proper-agent-connection.cjs {#archived-root-level-test-files-proper-agent-connection.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### protected-spawn.cjs {#archived-root-level-test-files-protected-spawn.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### quick-cursor-test.cjs {#archived-root-level-test-files-quick-cursor-test.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### read_debug_logs.py {#archived-root-level-test-files-read_debug_logs.py}
      # 🐍 Python

    ### real_screenshot_test.py {#archived-root-level-test-files-real_screenshot_test.py}
      # 🐍 Python

    ### reload-browser.cjs {#archived-root-level-test-files-reload-browser.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### safe_integration_test.cjs {#archived-root-level-test-files-safe_integration_test.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### screenshot-and-center.cjs {#archived-root-level-test-files-screenshot-and-center.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### self-controlling-ai.cjs {#archived-root-level-test-files-self-controlling-ai.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### self-testing-spawn.cjs {#archived-root-level-test-files-self-testing-spawn.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### simple_canvas_debug.py {#archived-root-level-test-files-simple_canvas_debug.py}
      # 🐍 Python

    ### simple_screenshot_capture.py {#archived-root-level-test-files-simple_screenshot_capture.py}
      # 🐍 Python

    ### smart-ecosystem.cjs {#archived-root-level-test-files-smart-ecosystem.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### stream-commands.cjs {#archived-root-level-test-files-stream-commands.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test_browser_websocket.py {#archived-root-level-test-files-test_browser_websocket.py}
      # 🐍 Python

    ### test_bus_after_greeting.py {#archived-root-level-test-files-test_bus_after_greeting.py}
      # 🐍 Python

    ### test_fluent_api.cjs {#archived-root-level-test-files-test_fluent_api.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test_macro_commands.cjs {#archived-root-level-test-files-test_macro_commands.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test_screenshot_data_api.py {#archived-root-level-test-files-test_screenshot_data_api.py}
      # 🐍 Python

    ### test_simple_bus_command.py {#archived-root-level-test-files-test_simple_bus_command.py}
      # 🐍 Python

    ### test-ai-connection.html {#archived-root-level-test-files-test-ai-connection.html}
      # 🗃️ Archived HTML (stored away)

    ### test-ai-cursor.cjs {#archived-root-level-test-files-test-ai-cursor.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-continuon-demo.cjs {#archived-root-level-test-files-test-continuon-demo.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-mouse-control.cjs {#archived-root-level-test-files-test-mouse-control.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-persistent-storage.cjs {#archived-root-level-test-files-test-persistent-storage.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-tab-focus.cjs {#archived-root-level-test-files-test-tab-focus.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-tab-registration.cjs {#archived-root-level-test-files-test-tab-registration.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-version-endpoint.cjs {#archived-root-level-test-files-test-version-endpoint.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-web-browser-demo.cjs {#archived-root-level-test-files-test-web-browser-demo.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### test-websocket-client.cjs {#archived-root-level-test-files-test-websocket-client.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### trigger-planner-training.cjs {#archived-root-level-test-files-trigger-planner-training.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### validate_milestone_debugger.py {#archived-root-level-test-files-validate_milestone_debugger.py}
      # 🐍 Python

    ### version_badge_screenshot.py {#archived-root-level-test-files-version_badge_screenshot.py}
      # 🐍 Python

    ### visual-control-module.cjs {#archived-root-level-test-files-visual-control-module.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### websocket-queue-test.cjs {#archived-root-level-test-files-websocket-queue-test.cjs}
      # 🗄️ Archived JavaScript (filed away)

    ### working_milestone_debugger.py {#archived-root-level-test-files-working_milestone_debugger.py}
      # 🐍 Python

    ### write_debug_logs.py {#archived-root-level-test-files-write_debug_logs.py}
      # 🐍 Python

      📁 **archived/root-level/test-files/ai-iterative-tests/**
      📁 **archived/root-level/test-files/ai-verification-tests/**
      ### config.txt {#archived-root-level-test-files-ai-verification-tests-config.txt}
        # 🗂️ Archived file (shelved)

      ### count.txt {#archived-root-level-test-files-ai-verification-tests-count.txt}
        # 🗂️ Archived file (shelved)

      ### date-test.txt {#archived-root-level-test-files-ai-verification-tests-date-test.txt}
        # 🗂️ Archived file (shelved)

      ### location.txt {#archived-root-level-test-files-ai-verification-tests-location.txt}
        # 🗂️ Archived file (shelved)

      ### system-info.txt {#archived-root-level-test-files-ai-verification-tests-system-info.txt}
        # 🗂️ Archived file (shelved)

        📁 **archived/root-level/test-files/ai-verification-tests/test-folder/**
        ### readme.md {#archived-root-level-test-files-ai-verification-tests-test-folder-readme.md}
          # 📦 Archived documentation

      📁 **archived/root-level/test-files/tests-directory/**
      ### academy-fine-tuning.test.cjs {#archived-root-level-test-files-tests-directory-academy-fine-tuning.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### adapter-sharing.test.cjs {#archived-root-level-test-files-tests-directory-adapter-sharing.test.cjs} 🌐 **TREASURE**
        # 🗄️ Archived JavaScript (filed away) - **360 lines of torrent-style sharing system**
        # 🔥 PROOF: Complete adapter sharing workflow with registry, export/import, installation
        # 🚀 FEATURES: Publish adapters, search by tags, cross-system sharing, 190,735x efficiency
        # 💎 DEMONSTRATES: "Share specialized fine-tuning in KB, not GB"

      ### adversarial-protocol.test.cjs {#archived-root-level-test-files-tests-directory-adversarial-protocol.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### basic.cjs {#archived-root-level-test-files-tests-directory-basic.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### build-system.test.cjs {#archived-root-level-test-files-tests-directory-build-system.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### command-processing.test.cjs {#archived-root-level-test-files-tests-directory-command-processing.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### complete-system-demo.cjs {#archived-root-level-test-files-tests-directory-complete-system-demo.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### comprehensive-api-test.cjs {#archived-root-level-test-files-tests-directory-comprehensive-api-test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### continuum-hierarchy.test.cjs {#archived-root-level-test-files-tests-directory-continuum-hierarchy.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### cyberpunk-theme.test.cjs {#archived-root-level-test-files-tests-directory-cyberpunk-theme.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### hierarchical-specialization.test.cjs {#archived-root-level-test-files-tests-directory-hierarchical-specialization.test.cjs} 🏆 **CROWN JEWEL**
        # 🗄️ Archived JavaScript (filed away) - **371 lines of hierarchical specialization proof**
        # 🧠 CORE SYSTEM: Demonstrates base → legal → patent → uspto → biotech stacking  
        # 🎯 REAL EXAMPLES: Legal(29MB), Medical(31MB), Engineering(27MB) complete stacks
        # 🔬 IMPLEMENTATION: Creates, loads, stacks, and applies hierarchical adapters
        # 💡 BREAKTHROUGH: "Perfect Architecture for Professional Specialization!"

      ### integration-full-system.test.ts {#archived-root-level-test-files-tests-directory-integration-full-system.test.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### integration.test.cjs {#archived-root-level-test-files-tests-directory-integration.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### lora-fine-tuning.test.cjs {#archived-root-level-test-files-tests-directory-lora-fine-tuning.test.cjs} 🎓 **ACADEMY PROOF**
        # 🗄️ Archived JavaScript (filed away) - **LoRA fine-tuning system implementation**
        # 🔬 FOUNDATION: Ultra-efficient fine-tuning for Academy adversarial training
        # 💾 EFFICIENCY: 190,735x parameter reduction enabling hierarchical stacking

      ### master-test-runner.cjs {#archived-root-level-test-files-tests-directory-master-test-runner.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### memory-package.test.cjs {#archived-root-level-test-files-tests-directory-memory-package.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### model-adapter-pricing.test.cjs {#archived-root-level-test-files-tests-directory-model-adapter-pricing.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### modular-system.test.cjs {#archived-root-level-test-files-tests-directory-modular-system.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### orchestrator.test.ts {#archived-root-level-test-files-tests-directory-orchestrator.test.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### performance.test.cjs {#archived-root-level-test-files-tests-directory-performance.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### persona-hierarchy-storage.test.cjs {#archived-root-level-test-files-tests-directory-persona-hierarchy-storage.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### persona-lifecycle.test.cjs {#archived-root-level-test-files-tests-directory-persona-lifecycle.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### prevent-claude-communication-errors.test.ts {#archived-root-level-test-files-tests-directory-prevent-claude-communication-errors.test.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### prevent-constant-reassignment.test.ts {#archived-root-level-test-files-tests-directory-prevent-constant-reassignment.test.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### protocol-sheriff.test.cjs {#archived-root-level-test-files-tests-directory-protocol-sheriff.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### screenshot-command.test.cjs {#archived-root-level-test-files-tests-directory-screenshot-command.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### security.test.cjs {#archived-root-level-test-files-tests-directory-security.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### self-validation.test.cjs {#archived-root-level-test-files-tests-directory-self-validation.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

      ### unit.test.cjs {#archived-root-level-test-files-tests-directory-unit.test.cjs}
        # 🗄️ Archived JavaScript (filed away)

        📁 **archived/root-level/test-files/tests-directory/communication/**
        ### AgentChannels.test.ts {#archived-root-level-test-files-tests-directory-communication-agentchannels.test.ts}
          # 🔗 TypeScript (missing from JS loop?)

        📁 **archived/root-level/test-files/tests-directory/integration/**
        ### console-logs.test.cjs {#archived-root-level-test-files-tests-directory-integration-console-logs.test.cjs}
          # 🗄️ Archived JavaScript (filed away)

        ### ContinuumChannels.test.ts {#archived-root-level-test-files-tests-directory-integration-continuumchannels.test.ts}
          # 🔗 TypeScript (missing from JS loop?)

        📁 **archived/root-level/test-files/tests-directory/system/**
        ### BasicTaskTests.test.ts {#archived-root-level-test-files-tests-directory-system-basictasktests.test.ts}
          # 🔗 TypeScript (missing from JS loop?)

        📁 **archived/root-level/test-files/tests-directory/tmp-core-test/**
          📁 **archived/root-level/test-files/tests-directory/tmp-core-test/test-project/**
        📁 **archived/root-level/test-files/tests-directory/ui/**
        ### ActionTracker.test.ts {#archived-root-level-test-files-tests-directory-ui-actiontracker.test.ts}
          # 🔗 TypeScript (missing from JS loop?)

        ### StatusIndicator.test.ts {#archived-root-level-test-files-tests-directory-ui-statusindicator.test.ts}
          # 🔗 TypeScript (missing from JS loop?)

        ### UIController.test.ts {#archived-root-level-test-files-tests-directory-ui-uicontroller.test.ts}
          # 🔗 TypeScript (missing from JS loop?)


📁 **assets/**
  📁 **assets/icons/**

📁 **coverage/**
### clover.xml {#coverage-clover.xml}
  # 📄 File

### coverage-final.json {#coverage-coverage-final.json}
  # 📋 Configuration/Data

### lcov.info {#coverage-lcov.info}
  # 📄 File

  📁 **coverage/lcov-report/**
  ### base.css {#coverage-lcov-report-base.css}
    # 🧤 File debris (spring cleaning time?)

  ### block-navigation.js {#coverage-lcov-report-block-navigation.js}
    # 🧽 JavaScript debris (cleanup needed?)

  ### index.html {#coverage-lcov-report-index.html}
    # 🧼 HTML scraps (temp/debug files?)

  ### prettify.css {#coverage-lcov-report-prettify.css}
    # 🧤 File debris (spring cleaning time?)

  ### prettify.js {#coverage-lcov-report-prettify.js}
    # 🧽 JavaScript debris (cleanup needed?)

  ### sorter.js {#coverage-lcov-report-sorter.js}
    # 🧽 JavaScript debris (cleanup needed?)

    📁 **coverage/lcov-report/cli/**
      📁 **coverage/lcov-report/cli/src/**
      ### ask.js.html {#coverage-lcov-report-cli-src-ask.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### context.js.html {#coverage-lcov-report-cli-src-context.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-cli-src-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.ts.html {#coverage-lcov-report-cli-src-index.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### templates.ts.html {#coverage-lcov-report-cli-src-templates.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### types.d.ts.html {#coverage-lcov-report-cli-src-types.d.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/cli/src/adapters/**
        ### claude.ts.html {#coverage-lcov-report-cli-src-adapters-claude.ts.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### gpt.ts.html {#coverage-lcov-report-cli-src-adapters-gpt.ts.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.html {#coverage-lcov-report-cli-src-adapters-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.ts.html {#coverage-lcov-report-cli-src-adapters-index.ts.html}
          # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/cli/src/commands/**
        ### adapt.ts.html {#coverage-lcov-report-cli-src-commands-adapt.ts.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.html {#coverage-lcov-report-cli-src-commands-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### init.ts.html {#coverage-lcov-report-cli-src-commands-init.ts.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### validate.ts.html {#coverage-lcov-report-cli-src-commands-validate.ts.html}
          # 🧼 HTML scraps (temp/debug files?)

    📁 **coverage/lcov-report/core/**
      📁 **coverage/lcov-report/core/src/**
      ### index.html {#coverage-lcov-report-core-src-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.ts.html {#coverage-lcov-report-core-src-index.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### types.ts.html {#coverage-lcov-report-core-src-types.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### utils.ts.html {#coverage-lcov-report-core-src-utils.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

    📁 **coverage/lcov-report/memory/**
      📁 **coverage/lcov-report/memory/src/**
      ### index.html {#coverage-lcov-report-memory-src-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.ts.html {#coverage-lcov-report-memory-src-index.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

    📁 **coverage/lcov-report/src/**
    ### index.html {#coverage-lcov-report-src-index.html}
      # 🧼 HTML scraps (temp/debug files?)

    ### intelligent-routing.cjs.html {#coverage-lcov-report-src-intelligent-routing.cjs.html}
      # 🧼 HTML scraps (temp/debug files?)

    ### orchestrator.ts.html {#coverage-lcov-report-src-orchestrator.ts.html}
      # 🧼 HTML scraps (temp/debug files?)

    ### process-manager.cjs.html {#coverage-lcov-report-src-process-manager.cjs.html}
      # 🧼 HTML scraps (temp/debug files?)

    ### self-improving-router.cjs.html {#coverage-lcov-report-src-self-improving-router.cjs.html}
      # 🧼 HTML scraps (temp/debug files?)

    ### tmux-claude-pool.cjs.html {#coverage-lcov-report-src-tmux-claude-pool.cjs.html}
      # 🧼 HTML scraps (temp/debug files?)

    ### working-web-interface.cjs.html {#coverage-lcov-report-src-working-web-interface.cjs.html}
      # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/adapters/**
      ### AdapterRegistry.cjs.html {#coverage-lcov-report-src-adapters-adapterregistry.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### BrowserAdapter.cjs.html {#coverage-lcov-report-src-adapters-browseradapter.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### HierarchicalAdapter.cjs.html {#coverage-lcov-report-src-adapters-hierarchicaladapter.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-adapters-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### LoRAAdapter.cjs.html {#coverage-lcov-report-src-adapters-loraadapter.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ModelAdapter.cjs.html {#coverage-lcov-report-src-adapters-modeladapter.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/agents/**
      ### Agent.ts.html {#coverage-lcov-report-src-agents-agent.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### AgentFactory.ts.html {#coverage-lcov-report-src-agents-agentfactory.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### base-agent.js.html {#coverage-lcov-report-src-agents-base-agent.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-agents-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### planner-ai.js.html {#coverage-lcov-report-src-agents-planner-ai.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ScreenshotAgent.cjs.html {#coverage-lcov-report-src-agents-screenshotagent.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/commands/**
      ### BaseCommand.cjs.html {#coverage-lcov-report-src-commands-basecommand.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### CommandRegistry.cjs.html {#coverage-lcov-report-src-commands-commandregistry.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-commands-index.html}
        # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/src/commands/core/**
        ### index.html {#coverage-lcov-report-src-commands-core-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### test-runner.cjs.html {#coverage-lcov-report-src-commands-core-test-runner.cjs.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### validation-test.cjs.html {#coverage-lcov-report-src-commands-core-validation-test.cjs.html}
          # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/agents/**
          ### AgentsCommand.cjs.html {#coverage-lcov-report-src-commands-core-agents-agentscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-agents-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-agents-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/browser/**
          ### BrowserCommand.cjs.html {#coverage-lcov-report-src-commands-core-browser-browsercommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-browser-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-browser-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/browserjs/**
          ### BrowserJSCommand.cjs.html {#coverage-lcov-report-src-commands-core-browserjs-browserjscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-browserjs-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-browserjs-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/chat/**
          ### ChatCommand.cjs.html {#coverage-lcov-report-src-commands-core-chat-chatcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-chat-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-chat-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/clear/**
          ### ClearCommand.cjs.html {#coverage-lcov-report-src-commands-core-clear-clearcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-clear-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-clear-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/createroom/**
          ### CreateRoomCommand.cjs.html {#coverage-lcov-report-src-commands-core-createroom-createroomcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-createroom-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-createroom-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/cursor/**
          ### CursorCommand.cjs.html {#coverage-lcov-report-src-commands-core-cursor-cursorcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-cursor-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-cursor-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

            📁 **coverage/lcov-report/src/commands/core/cursor/graphics/**
            ### GraphicsRenderer.js.html {#coverage-lcov-report-src-commands-core-cursor-graphics-graphicsrenderer.js.html}
              # 🧼 HTML scraps (temp/debug files?)

            ### index.html {#coverage-lcov-report-src-commands-core-cursor-graphics-index.html}
              # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/diagnostics/**
          ### DiagnosticsCommand.cjs.html {#coverage-lcov-report-src-commands-core-diagnostics-diagnosticscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.cjs.html {#coverage-lcov-report-src-commands-core-diagnostics-index.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-diagnostics-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-diagnostics-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/emotion/**
          ### EmotionCommand.cjs.html {#coverage-lcov-report-src-commands-core-emotion-emotioncommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### emotionConfigs.cjs.html {#coverage-lcov-report-src-commands-core-emotion-emotionconfigs.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### emotionConfigs.js.html {#coverage-lcov-report-src-commands-core-emotion-emotionconfigs.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### emotionDefinition.cjs.html {#coverage-lcov-report-src-commands-core-emotion-emotiondefinition.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-emotion-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-emotion-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/exec/**
          ### ExecCommand.cjs.html {#coverage-lcov-report-src-commands-core-exec-execcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-exec-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-exec-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/fileSave/**
          ### FileSaveCommand.cjs.html {#coverage-lcov-report-src-commands-core-filesave-filesavecommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-filesave-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-filesave-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/findUser/**
          ### FindUserCommand.cjs.html {#coverage-lcov-report-src-commands-core-finduser-findusercommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.cjs.html {#coverage-lcov-report-src-commands-core-finduser-index.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-finduser-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-finduser-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/help/**
          ### HelpCommand.cjs.html {#coverage-lcov-report-src-commands-core-help-helpcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.cjs.html {#coverage-lcov-report-src-commands-core-help-index.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-help-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-help-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/info/**
          ### index.html {#coverage-lcov-report-src-commands-core-info-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-info-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### InfoCommand.cjs.html {#coverage-lcov-report-src-commands-core-info-infocommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/input/**
          ### index.html {#coverage-lcov-report-src-commands-core-input-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-input-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### InputCommand.cjs.html {#coverage-lcov-report-src-commands-core-input-inputcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/joinroom/**
          ### index.html {#coverage-lcov-report-src-commands-core-joinroom-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-joinroom-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### JoinRoomCommand.cjs.html {#coverage-lcov-report-src-commands-core-joinroom-joinroomcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/listagents/**
          ### index.html {#coverage-lcov-report-src-commands-core-listagents-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-listagents-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ListAgentsCommand.cjs.html {#coverage-lcov-report-src-commands-core-listagents-listagentscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/listrooms/**
          ### index.html {#coverage-lcov-report-src-commands-core-listrooms-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ListRoomsCommand.cjs.html {#coverage-lcov-report-src-commands-core-listrooms-listroomscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/loadrooms/**
          ### index.html {#coverage-lcov-report-src-commands-core-loadrooms-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-loadrooms-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### LoadRoomsCommand.cjs.html {#coverage-lcov-report-src-commands-core-loadrooms-loadroomscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/macro/**
          ### index.html {#coverage-lcov-report-src-commands-core-macro-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-macro-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### MacroCommand.cjs.html {#coverage-lcov-report-src-commands-core-macro-macrocommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/move/**
          ### index.html {#coverage-lcov-report-src-commands-core-move-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-move-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### MoveCommand.cjs.html {#coverage-lcov-report-src-commands-core-move-movecommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/preferences/**
          ### index.html {#coverage-lcov-report-src-commands-core-preferences-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-preferences-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### PreferencesCommand.cjs.html {#coverage-lcov-report-src-commands-core-preferences-preferencescommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/promisejs/**
          ### index.html {#coverage-lcov-report-src-commands-core-promisejs-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-promisejs-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### PromiseJSCommand.cjs.html {#coverage-lcov-report-src-commands-core-promisejs-promisejscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/reload/**
          ### index.html {#coverage-lcov-report-src-commands-core-reload-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-reload-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ReloadCommand.cjs.html {#coverage-lcov-report-src-commands-core-reload-reloadcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/restart/**
          ### index.html {#coverage-lcov-report-src-commands-core-restart-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-restart-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### RestartCommand.cjs.html {#coverage-lcov-report-src-commands-core-restart-restartcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/savefile/**
          ### index.html {#coverage-lcov-report-src-commands-core-savefile-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-savefile-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### SaveFileCommand.cjs.html {#coverage-lcov-report-src-commands-core-savefile-savefilecommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/screenshot/**
          ### ContinuonAnimator.js.html {#coverage-lcov-report-src-commands-core-screenshot-continuonanimator.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.cjs.html {#coverage-lcov-report-src-commands-core-screenshot-index.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.client.js.html {#coverage-lcov-report-src-commands-core-screenshot-index.client.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-screenshot-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-screenshot-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ScreenshotCommand.cjs.html {#coverage-lcov-report-src-commands-core-screenshot-screenshotcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ScreenshotCommand.client.js.html {#coverage-lcov-report-src-commands-core-screenshot-screenshotcommand.client.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ScreenshotUtils.js.html {#coverage-lcov-report-src-commands-core-screenshot-screenshotutils.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/share/**
          ### index.cjs.html {#coverage-lcov-report-src-commands-core-share-index.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-share-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-share-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ShareCommand.cjs.html {#coverage-lcov-report-src-commands-core-share-sharecommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/test/**
          ### index.cjs.html {#coverage-lcov-report-src-commands-core-test-index.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.html {#coverage-lcov-report-src-commands-core-test-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-test-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### TestCommand.cjs.html {#coverage-lcov-report-src-commands-core-test-testcommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/type/**
          ### index.html {#coverage-lcov-report-src-commands-core-type-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-type-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### TypeCommand.cjs.html {#coverage-lcov-report-src-commands-core-type-typecommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/validatecode/**
          ### index.html {#coverage-lcov-report-src-commands-core-validatecode-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-validatecode-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ValidateCodeCommand.cjs.html {#coverage-lcov-report-src-commands-core-validatecode-validatecodecommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

          📁 **coverage/lcov-report/src/commands/core/validatejs/**
          ### index.html {#coverage-lcov-report-src-commands-core-validatejs-index.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### index.server.js.html {#coverage-lcov-report-src-commands-core-validatejs-index.server.js.html}
            # 🧼 HTML scraps (temp/debug files?)

          ### ValidateJSCommand.cjs.html {#coverage-lcov-report-src-commands-core-validatejs-validatejscommand.cjs.html}
            # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/core/**
      ### Academy.cjs.html {#coverage-lcov-report-src-core-academy.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### AdversarialPair.cjs.html {#coverage-lcov-report-src-core-adversarialpair.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### AICapabilityMatcher.cjs.html {#coverage-lcov-report-src-core-aicapabilitymatcher.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### AIModel.cjs.html {#coverage-lcov-report-src-core-aimodel.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### BrowserLogger.cjs.html {#coverage-lcov-report-src-core-browserlogger.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### CommandDefinitions.cjs.html {#coverage-lcov-report-src-core-commanddefinitions.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### CommandProcessor.cjs.html {#coverage-lcov-report-src-core-commandprocessor.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### CommandTeacher.cjs.html {#coverage-lcov-report-src-core-commandteacher.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### continuum-core.cjs.html {#coverage-lcov-report-src-core-continuum-core.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### CostTracker.cjs.html {#coverage-lcov-report-src-core-costtracker.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### FineTuningDataGenerator.cjs.html {#coverage-lcov-report-src-core-finetuningdatagenerator.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### GameTrainer.cjs.html {#coverage-lcov-report-src-core-gametrainer.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-core-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### MessageQueue.cjs.html {#coverage-lcov-report-src-core-messagequeue.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ModelCaliber.cjs.html {#coverage-lcov-report-src-core-modelcaliber.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### Persona.cjs.html {#coverage-lcov-report-src-core-persona.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### PersonaBootcamp.cjs.html {#coverage-lcov-report-src-core-personabootcamp.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### PersonaFactory.cjs.html {#coverage-lcov-report-src-core-personafactory.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### PersonaLibrary.cjs.html {#coverage-lcov-report-src-core-personalibrary.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### PersonaRegistry.cjs.html {#coverage-lcov-report-src-core-personaregistry.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ProtocolSheriff.cjs.html {#coverage-lcov-report-src-core-protocolsheriff.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### RequestManagerDroid.cjs.html {#coverage-lcov-report-src-core-requestmanagerdroid.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### SheriffTrainer.cjs.html {#coverage-lcov-report-src-core-sherifftrainer.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### TestingDroid.cjs.html {#coverage-lcov-report-src-core-testingdroid.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ValidationPipeline.cjs.html {#coverage-lcov-report-src-core-validationpipeline.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### VersionManager.cjs.html {#coverage-lcov-report-src-core-versionmanager.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/integrations/**
      ### ContinuonRing.cjs.html {#coverage-lcov-report-src-integrations-continuonring.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ContinuonTray.cjs.html {#coverage-lcov-report-src-integrations-continuontray.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### github-ci.cjs.html {#coverage-lcov-report-src-integrations-github-ci.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### HttpServer.cjs.html {#coverage-lcov-report-src-integrations-httpserver.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-integrations-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### MacOSMenuBar.cjs.html {#coverage-lcov-report-src-integrations-macosmenubar.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ScreenshotIntegration.cjs.html {#coverage-lcov-report-src-integrations-screenshotintegration.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### SimpleMenuBar.cjs.html {#coverage-lcov-report-src-integrations-simplemenubar.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### SystemTray.cjs.html {#coverage-lcov-report-src-integrations-systemtray.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### SystemTraySimple.cjs.html {#coverage-lcov-report-src-integrations-systemtraysimple.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### WebSocketServer.cjs.html {#coverage-lcov-report-src-integrations-websocketserver.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/interfaces/**
      ### agent-interface.js.html {#coverage-lcov-report-src-interfaces-agent-interface.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### agent.interface.ts.html {#coverage-lcov-report-src-interfaces-agent.interface.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-interfaces-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### tool-interface.js.html {#coverage-lcov-report-src-interfaces-tool-interface.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/modules/**
      ### CommandModule.cjs.html {#coverage-lcov-report-src-modules-commandmodule.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### CoreModule.cjs.html {#coverage-lcov-report-src-modules-coremodule.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### FluentAPI.cjs.html {#coverage-lcov-report-src-modules-fluentapi.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-modules-index.html}
        # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/src/modules/ui/**
        ### AgentSelector.js.html {#coverage-lcov-report-src-modules-ui-agentselector.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.html {#coverage-lcov-report-src-modules-ui-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### ScreenshotFeedback.js.html {#coverage-lcov-report-src-modules-ui-screenshotfeedback.js.html}
          # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/services/**
      ### CommandDiscoveryService.cjs.html {#coverage-lcov-report-src-services-commanddiscoveryservice.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### GameManager.cjs.html {#coverage-lcov-report-src-services-gamemanager.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-services-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ModelDiscoveryService.js.html {#coverage-lcov-report-src-services-modeldiscoveryservice.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ModelDiscoveryService.ts.html {#coverage-lcov-report-src-services-modeldiscoveryservice.ts.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### RemoteAgentManager.cjs.html {#coverage-lcov-report-src-services-remoteagentmanager.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ScreenshotService.cjs.html {#coverage-lcov-report-src-services-screenshotservice.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### TabManager.cjs.html {#coverage-lcov-report-src-services-tabmanager.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### VisualGameManager.cjs.html {#coverage-lcov-report-src-services-visualgamemanager.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### WebVisualManager.cjs.html {#coverage-lcov-report-src-services-webvisualmanager.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/storage/**
      ### index.html {#coverage-lcov-report-src-storage-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### ModelCheckpoint.cjs.html {#coverage-lcov-report-src-storage-modelcheckpoint.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### PersistentStorage.cjs.html {#coverage-lcov-report-src-storage-persistentstorage.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/src/storage/persistent/**
        ### index.html {#coverage-lcov-report-src-storage-persistent-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.server.js.html {#coverage-lcov-report-src-storage-persistent-index.server.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### PersistentStorage.cjs.html {#coverage-lcov-report-src-storage-persistent-persistentstorage.cjs.html}
          # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/tests/**
      ### demo-graceful-shutdown.cjs.html {#coverage-lcov-report-src-tests-demo-graceful-shutdown.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-tests-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### run-all-tests.cjs.html {#coverage-lcov-report-src-tests-run-all-tests.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-agent-channels.cjs.html {#coverage-lcov-report-src-tests-test-agent-channels.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-ai-basic-tasks.cjs.html {#coverage-lcov-report-src-tests-test-ai-basic-tasks.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-ai-file-operations.cjs.html {#coverage-lcov-report-src-tests-test-ai-file-operations.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-ai-greeting.cjs.html {#coverage-lcov-report-src-tests-test-ai-greeting.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-ai-iterative.cjs.html {#coverage-lcov-report-src-tests-test-ai-iterative.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-ai-verifiable.cjs.html {#coverage-lcov-report-src-tests-test-ai-verifiable.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-ai-with-tools.cjs.html {#coverage-lcov-report-src-tests-test-ai-with-tools.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-continuum-spawn.cjs.html {#coverage-lcov-report-src-tests-test-continuum-spawn.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-continuum-system.cjs.html {#coverage-lcov-report-src-tests-test-continuum-system.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-continuum-web.cjs.html {#coverage-lcov-report-src-tests-test-continuum-web.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-everything.cjs.html {#coverage-lcov-report-src-tests-test-everything.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-graceful-shutdown.cjs.html {#coverage-lcov-report-src-tests-test-graceful-shutdown.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-interactive.cjs.html {#coverage-lcov-report-src-tests-test-interactive.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-live-continuum.cjs.html {#coverage-lcov-report-src-tests-test-live-continuum.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-real-ai-intelligence.cjs.html {#coverage-lcov-report-src-tests-test-real-ai-intelligence.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-real-interaction.cjs.html {#coverage-lcov-report-src-tests-test-real-interaction.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-real-pool.cjs.html {#coverage-lcov-report-src-tests-test-real-pool.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-routing-logic.cjs.html {#coverage-lcov-report-src-tests-test-routing-logic.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-send-function.cjs.html {#coverage-lcov-report-src-tests-test-send-function.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-shell-call.cjs.html {#coverage-lcov-report-src-tests-test-shell-call.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-simple-ai.cjs.html {#coverage-lcov-report-src-tests-test-simple-ai.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-smart-integration.cjs.html {#coverage-lcov-report-src-tests-test-smart-integration.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-tool-execution.cjs.html {#coverage-lcov-report-src-tests-test-tool-execution.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### test-working-pool.cjs.html {#coverage-lcov-report-src-tests-test-working-pool.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/tools/**
      ### index.html {#coverage-lcov-report-src-tools-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### PromiseJSExecutor.cjs.html {#coverage-lcov-report-src-tools-promisejsexecutor.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### web-fetch-tool.js.html {#coverage-lcov-report-src-tools-web-fetch-tool.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      📁 **coverage/lcov-report/src/ui/**
      ### AcademyWebInterface.cjs.html {#coverage-lcov-report-src-ui-academywebinterface.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### command-handler.js.html {#coverage-lcov-report-src-ui-command-handler.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### continuum-api.js.html {#coverage-lcov-report-src-ui-continuum-api.js.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### index.html {#coverage-lcov-report-src-ui-index.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### UIGenerator.cjs.html {#coverage-lcov-report-src-ui-uigenerator.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### UIGeneratorModular.cjs.html {#coverage-lcov-report-src-ui-uigeneratormodular.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

      ### WebComponentsIntegration.cjs.html {#coverage-lcov-report-src-ui-webcomponentsintegration.cjs.html}
        # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/src/ui/components/**
        ### AcademySection.js.html {#coverage-lcov-report-src-ui-components-academysection.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### AgentSelector.js.html {#coverage-lcov-report-src-ui-components-agentselector.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### AIWidget.js.html {#coverage-lcov-report-src-ui-components-aiwidget.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### ChatArea.js.html {#coverage-lcov-report-src-ui-components-chatarea.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### ChatHeader.js.html {#coverage-lcov-report-src-ui-components-chatheader.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### GlassMenu.js.html {#coverage-lcov-report-src-ui-components-glassmenu.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.html {#coverage-lcov-report-src-ui-components-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### RoomTabs.js.html {#coverage-lcov-report-src-ui-components-roomtabs.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### SimpleAgentSelector.js.html {#coverage-lcov-report-src-ui-components-simpleagentselector.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### StatusPill.js.html {#coverage-lcov-report-src-ui-components-statuspill.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### UserDrawer.js.html {#coverage-lcov-report-src-ui-components-userdrawer.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/src/ui/utils/**
        ### AgentSelectorUtils.js.html {#coverage-lcov-report-src-ui-utils-agentselectorutils.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### ComponentLoader.js.html {#coverage-lcov-report-src-ui-utils-componentloader.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.html {#coverage-lcov-report-src-ui-utils-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        📁 **coverage/lcov-report/src/ui/widgets/**
        ### AgentWidget.js.html {#coverage-lcov-report-src-ui-widgets-agentwidget.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### BaseConnectionWidget.js.html {#coverage-lcov-report-src-ui-widgets-baseconnectionwidget.js.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### index.html {#coverage-lcov-report-src-ui-widgets-index.html}
          # 🧼 HTML scraps (temp/debug files?)

        ### UnifiedSlideoutPanel.js.html {#coverage-lcov-report-src-ui-widgets-unifiedslideoutpanel.js.html}
          # 🧼 HTML scraps (temp/debug files?)


📁 **docs/**
### ACADEMY_ARCHITECTURE.md {#docs-academy_architecture.md}
  # 📖 Documentation

### AGENT_DEVELOPMENT_GUIDE.md {#docs-agent_development_guide.md}
  # 📖 Documentation

### AI_PORTAL_ARCHITECTURE.md {#docs-ai_portal_architecture.md}
  # 📖 Documentation

### DEBUGGING_UTILITIES.md {#docs-debugging_utilities.md}
  # 📖 Documentation

### UNIVERSAL_COMMAND_ARCHITECTURE.md {#docs-universal_command_architecture.md}
  # 📖 Documentation

  📁 **docs/images/**
  ### academy-training-room.png {#docs-images-academy-training-room.png}
    # 📄 File

  ### continuum-main-interface.png {#docs-images-continuum-main-interface.png}
    # 📄 File


📁 **examples/**
### continuum.claude {#examples-continuum.claude}
  # 📄 File

### continuum.gpt {#examples-continuum.gpt}
  # 📄 File

### package.json {#examples-package.json}
  # 📋 Configuration/Data

### README.md {#examples-readme.md}
  # 📖 Documentation

### test-fred-agent.cjs {#examples-test-fred-agent.cjs}
  # ⚡ JavaScript/Node.js

### visualize-config-simple.js {#examples-visualize-config-simple.js}
  # ⚡ JavaScript/Node.js

### visualize-config.js {#examples-visualize-config.js}
  # ⚡ JavaScript/Node.js

  📁 **examples/claude/**
  ### CLAUDE.md {#examples-claude-claude.md}
    # 📖 Documentation

  📁 **examples/gpt/**
  ### system_prompt.txt {#examples-gpt-system_prompt.txt}
    # 📄 File


📁 **externals/**
  📁 **externals/continuum/**
    📁 **externals/continuum/packages/**
      📁 **externals/continuum/packages/web-tester/**
        📁 **externals/continuum/packages/web-tester/src/**

📁 **packages/**
  📁 **packages/adapters/**
  ### package.json {#packages-adapters-package.json}
    # 📋 Configuration/Data

  📁 **packages/cli/**
  ### package.json {#packages-cli-package.json}
    # 📋 Configuration/Data

  ### tsconfig.json {#packages-cli-tsconfig.json}
    # 📋 Configuration/Data

    📁 **packages/cli/__tests__/**
    ### cli.test.ts {#packages-cli-__tests__-cli.test.ts}
      # 🔗 TypeScript (missing from JS loop?)

      📁 **packages/cli/__tests__/commands/**
      ### adapt.test.ts {#packages-cli-__tests__-commands-adapt.test.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### init.test.ts {#packages-cli-__tests__-commands-init.test.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### validate.test.ts {#packages-cli-__tests__-commands-validate.test.ts}
        # 🔗 TypeScript (missing from JS loop?)

    📁 **packages/cli/bin/**
    ### continuum.js {#packages-cli-bin-continuum.js}
      # ⚡ JavaScript/Node.js

    📁 **packages/cli/src/**
    ### ask.js {#packages-cli-src-ask.js}
      # ⚡ JavaScript/Node.js

    ### context.js {#packages-cli-src-context.js}
      # ⚡ JavaScript/Node.js

    ### index.ts {#packages-cli-src-index.ts}
      # 🔗 TypeScript (missing from JS loop?)

    ### templates.ts {#packages-cli-src-templates.ts}
      # 🔗 TypeScript (missing from JS loop?)

    ### types.d.ts {#packages-cli-src-types.d.ts}
      # 🔗 TypeScript (missing from JS loop?)

      📁 **packages/cli/src/adapters/**
      ### claude.ts {#packages-cli-src-adapters-claude.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### gpt.ts {#packages-cli-src-adapters-gpt.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### index.ts {#packages-cli-src-adapters-index.ts}
        # 🔗 TypeScript (missing from JS loop?)

      📁 **packages/cli/src/commands/**
      ### adapt.ts {#packages-cli-src-commands-adapt.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### init.ts {#packages-cli-src-commands-init.ts}
        # 🔗 TypeScript (missing from JS loop?)

      ### validate.ts {#packages-cli-src-commands-validate.ts}
        # 🔗 TypeScript (missing from JS loop?)

  📁 **packages/core/**
  ### package.json {#packages-core-package.json}
    # 📋 Configuration/Data

  ### tsconfig.json {#packages-core-tsconfig.json}
    # 📋 Configuration/Data

  ### tsconfig.tsbuildinfo {#packages-core-tsconfig.tsbuildinfo}
    # 📄 File

    📁 **packages/core/__tests__/**
    ### core.test.ts {#packages-core-__tests__-core.test.ts}
      # 🔗 TypeScript (missing from JS loop?)

    📁 **packages/core/src/**
    ### index.ts {#packages-core-src-index.ts}
      # 🔗 TypeScript (missing from JS loop?)

    ### types.ts {#packages-core-src-types.ts}
      # 🔗 TypeScript (missing from JS loop?)

    ### utils.ts {#packages-core-src-utils.ts}
      # 🔗 TypeScript (missing from JS loop?)

  📁 **packages/memory/**
  ### package.json {#packages-memory-package.json}
    # 📋 Configuration/Data

  ### tsconfig.json {#packages-memory-tsconfig.json}
    # 📋 Configuration/Data

    📁 **packages/memory/src/**
    ### index.ts {#packages-memory-src-index.ts}
      # 🔗 TypeScript (missing from JS loop?)

  📁 **packages/plugins/**
  ### package.json {#packages-plugins-package.json}
    # 📋 Configuration/Data

  📁 **packages/revenue/**
    📁 **packages/revenue/src/**
    ### cloud-deployment-ai.ts {#packages-revenue-src-cloud-deployment-ai.ts}
      # 🔗 TypeScript (missing from JS loop?)

    ### revenue-generation-ai.ts {#packages-revenue-src-revenue-generation-ai.ts}
      # 🔗 TypeScript (missing from JS loop?)

  📁 **packages/self-development/**
    📁 **packages/self-development/src/**
    ### continuum-developer-ai.ts {#packages-self-development-src-continuum-developer-ai.ts}
      # 🔗 TypeScript (missing from JS loop?)

    ### git-aware-developer.ts {#packages-self-development-src-git-aware-developer.ts}
      # 🔗 TypeScript (missing from JS loop?)

    ### self-improvement-coordinator.ts {#packages-self-development-src-self-improvement-coordinator.ts}
      # 🔗 TypeScript (missing from JS loop?)

  📁 **packages/web-tester/**
  ### continuum.log {#packages-web-tester-continuum.log}
    # 📄 File

  ### server.log {#packages-web-tester-server.log}
    # 📄 File

    📁 **packages/web-tester/output/**
      📁 **packages/web-tester/output/screenshots/**

📁 **python-client/**
### ai-agent-README.md {#python-client-ai-agent-readme.md}
  # 📖 Documentation

### ai-agent.py {#python-client-ai-agent.py}
  # 🐍 Python

### ai-portal.py {#python-client-ai-portal.py}
  # 🐍 Python

### git-dashboard-integration.py {#python-client-git-dashboard-integration.py}
  # 🐍 Python

### pytest.ini {#python-client-pytest.ini}
  # 📄 File

### README.md {#python-client-readme.md}
  # 📖 Documentation

### requirements.txt {#python-client-requirements.txt}
  # 📦 Python dependencies

### run-integration-tests.sh {#python-client-run-integration-tests.sh}
  # 🔧 Shell Script

### setup.py {#python-client-setup.py}
  # 🐍 Python

### simple_continuum_client.py {#python-client-simple_continuum_client.py}
  # 🐍 Python

### trust_the_process.py {#python-client-trust_the_process.py}
  # 🐍 Python

  📁 **python-client/claude_debugger/**
  ### __init__.py {#python-client-claude_debugger-__init__.py}
    # 🐍 Python

  ### main.py {#python-client-claude_debugger-main.py}
    # 🐍 Python

    📁 **python-client/claude_debugger/connection/**
    ### __init__.py {#python-client-claude_debugger-connection-__init__.py}
      # 🐍 Python

    ### websocket_connection.py {#python-client-claude_debugger-connection-websocket_connection.py}
      # 🐍 Python

    📁 **python-client/claude_debugger/managers/**
    ### __init__.py {#python-client-claude_debugger-managers-__init__.py}
      # 🐍 Python

    ### server_log_manager.py {#python-client-claude_debugger-managers-server_log_manager.py}
      # 🐍 Python

    📁 **python-client/claude_debugger/validation/**
    ### __init__.py {#python-client-claude_debugger-validation-__init__.py}
      # 🐍 Python

    ### connection_validator.py {#python-client-claude_debugger-validation-connection_validator.py}
      # 🐍 Python

    ### javascript_validator.py {#python-client-claude_debugger-validation-javascript_validator.py}
      # 🐍 Python

  📁 **python-client/continuum_client/**
  ### __init__.py {#python-client-continuum_client-__init__.py}
    # 🐍 Python

    📁 **python-client/continuum_client/core/**
    ### client.py {#python-client-continuum_client-core-client.py}
      # 🐍 Python

    ### command_interface.py {#python-client-continuum_client-core-command_interface.py}
      # 🐍 Python

    ### js_executor.py {#python-client-continuum_client-core-js_executor.py}
      # 🐍 Python

    📁 **python-client/continuum_client/diagnostics/**
    ### __init__.py {#python-client-continuum_client-diagnostics-__init__.py}
      # 🐍 Python

    ### self_diagnostics.py {#python-client-continuum_client-diagnostics-self_diagnostics.py}
      # 🐍 Python

    📁 **python-client/continuum_client/exceptions/**
    ### js_errors.py {#python-client-continuum_client-exceptions-js_errors.py}
      # 🐍 Python

    📁 **python-client/continuum_client/utils/**
    ### __init__.py {#python-client-continuum_client-utils-__init__.py}
      # 🐍 Python

    ### config.py {#python-client-continuum_client-utils-config.py}
      # 🐍 Python

    ### screenshot.py {#python-client-continuum_client-utils-screenshot.py}
      # 🐍 Python

    ### server_manager.py {#python-client-continuum_client-utils-server_manager.py}
      # 🐍 Python

  📁 **python-client/examples/**
  ### component_css_fixer.py {#python-client-examples-component_css_fixer.py}
    # 🐍 Python

  ### diagnose_component_issues.py {#python-client-examples-diagnose_component_issues.py}
    # 🐍 Python

  ### find_and_capture.py {#python-client-examples-find_and_capture.py}
    # 🐍 Python

  ### fix_and_test_glass_submenu.py {#python-client-examples-fix_and_test_glass_submenu.py}
    # 🐍 Python

  ### fix_ui_styling_with_feedback.py {#python-client-examples-fix_ui_styling_with_feedback.py}
    # 🐍 Python

  ### force_visible_glass_submenu.py {#python-client-examples-force_visible_glass_submenu.py}
    # 🐍 Python

  ### natural_glass_submenu_demo.py {#python-client-examples-natural_glass_submenu_demo.py}
    # 🐍 Python

  ### README_glass_submenu_demo.md {#python-client-examples-readme_glass_submenu_demo.md}
    # 📖 Documentation

  ### README_UI_STYLING_TOOLS.md {#python-client-examples-readme_ui_styling_tools.md}
    # 📖 Documentation

  ### README.md {#python-client-examples-readme.md}
    # 📖 Documentation

  ### test_glass_submenu_system.py {#python-client-examples-test_glass_submenu_system.py}
    # 🐍 Python

  ### ui_styling_debugger.py {#python-client-examples-ui_styling_debugger.py}
    # 🐍 Python

    📁 **python-client/examples/screenshots/**
  📁 **python-client/tests/**
  ### README.md {#python-client-tests-readme.md}
    # 📖 Documentation

    📁 **python-client/tests/fixtures/**
    ### __init__.py {#python-client-tests-fixtures-__init__.py}
      # 🐍 Python

    ### mock_server.py {#python-client-tests-fixtures-mock_server.py}
      # 🐍 Python

    📁 **python-client/tests/integration/**
    ### conftest.py {#python-client-tests-integration-conftest.py}
      # 🐍 Python

    ### test_crash_recovery.py {#python-client-tests-integration-test_crash_recovery.py}
      # 🐍 Python

    ### test_fred_registration.py {#python-client-tests-integration-test_fred_registration.py}
      # 🐍 Python

    ### test_full_flow.py {#python-client-tests-integration-test_full_flow.py}
      # 🐍 Python

    ### test_html_parsing.py {#python-client-tests-integration-test_html_parsing.py}
      # 🐍 Python

    ### test_js_promise_errors.py {#python-client-tests-integration-test_js_promise_errors.py}
      # 🐍 Python

    ### test_promise_flow.py {#python-client-tests-integration-test_promise_flow.py}
      # 🐍 Python

    ### test_ui_updates.py {#python-client-tests-integration-test_ui_updates.py}
      # 🐍 Python

    📁 **python-client/tests/unit/**
    ### test_ai_dashboard.py {#python-client-tests-unit-test_ai_dashboard.py}
      # 🐍 Python

    ### test_client.py {#python-client-tests-unit-test_client.py}
      # 🐍 Python

    ### test_js_executor.py {#python-client-tests-unit-test_js_executor.py}
      # 🐍 Python

    ### test_screenshot_utils.py {#python-client-tests-unit-test_screenshot_utils.py}
      # 🐍 Python


📁 **schema/**
### commands.schema.json {#schema-commands.schema.json}
  # 📋 Configuration/Data

### continuum.schema.json {#schema-continuum.schema.json}
  # 📋 Configuration/Data


📁 **scripts/**
### demo-persona-factory.cjs {#scripts-demo-persona-factory.cjs}
  # ⚡ JavaScript/Node.js

### generate-files-tree-safe.sh {#scripts-generate-files-tree-safe.sh}
  # 🔧 Shell Script

### generate-files-tree.sh {#scripts-generate-files-tree.sh}
  # 🔧 Shell Script

### run-academy.cjs {#scripts-run-academy.cjs}
  # 🎓 ACADEMY: Matrix-inspired adversarial training script - trains AI personas (sheriff-mahoney, officer-hightower) through TestingDroid vs ProtocolSheriff GAN-like boot camp

### test-ci.sh {#scripts-test-ci.sh}
  # 🔧 Shell Script

### train-planner-academy.cjs {#scripts-train-planner-academy.cjs}
  # ⚡ JavaScript/Node.js

### train-sheriff.cjs {#scripts-train-sheriff.cjs}
  # ⚡ JavaScript/Node.js

### update-files-tree.sh {#scripts-update-files-tree.sh}
  # 🔧 Shell Script

### update-lerna.sh {#scripts-update-lerna.sh}
  # 🔧 Shell Script

### validate-schema.js {#scripts-validate-schema.js}
  # ⚡ JavaScript/Node.js


📁 **src/**
### intelligent-routing.cjs {#src-intelligent-routing.cjs}
  # ⚡ JavaScript/Node.js

### orchestrator.ts {#src-orchestrator.ts}
  # 📄 File

### process-manager.cjs {#src-process-manager.cjs}
  # ⚡ JavaScript/Node.js

### self-improving-router.cjs {#src-self-improving-router.cjs}
  # ⚡ JavaScript/Node.js

### tmux-claude-pool.cjs {#src-tmux-claude-pool.cjs}
  # ⚡ JavaScript/Node.js

### working-web-interface.cjs {#src-working-web-interface.cjs}
  # ⚡ JavaScript/Node.js

  📁 **src/adapters/** 🎓 **CRITICAL ARCHAEOLOGICAL DISCOVERY**
  ### AdapterRegistry.cjs {#src-adapters-adapterregistry.cjs}
    # ⚡ JavaScript/Node.js - **Torrent-style adapter sharing system**
    # 🔥 FOUND: publishAdapter(), searchAdapters(), exportAdapter() methods
    # 💡 Enables decentralized expertise mesh between organizations

  ### BrowserAdapter.cjs {#src-adapters-browseradapter.cjs}
    # ⚡ JavaScript/Node.js

  ### HierarchicalAdapter.cjs {#src-adapters-hierarchicaladapter.cjs} 🏆 **TREASURE**
    # ⚡ JavaScript/Node.js - **331 lines of hierarchical specialization architecture**
    # 🧠 CORE SYSTEM: Composable expertise stacking (base → science → physics → biology)
    # 🚀 CAPABILITIES: 190,735x storage reduction, instant specialization swapping
    # 💎 EXAMPLES: Legal(29MB), Medical(31MB), Engineering(27MB) complete stacks
    # 🔗 INTEGRATION: Academy-trained layers, torrent-style sharing, privacy-preserving

  ### LoRAAdapter.cjs {#src-adapters-loraadapter.cjs} 🎓 **ACADEMY COMPONENT**
    # ⚡ JavaScript/Node.js - **Ultra-efficient fine-tuning system** 
    # 🔬 CORE: LoRA (Low-Rank Adaptation) for 190,735x parameter reduction
    # 🏋️ TRAINING: Base for Academy adversarial training system
    # 💾 EFFICIENCY: Tiny specialized layers instead of full model retraining

  ### ModelAdapter.cjs {#src-adapters-modeladapter.cjs}
    # ⚡ JavaScript/Node.js - **Multi-provider model integration**
    # 🤖 FACTORY: Creates model instances for different providers
    # 🔗 INTEGRATION: Works with LoRA and hierarchical adapters

  📁 **src/agents/**
  ### Agent.ts {#src-agents-agent.ts}
    # 🔗 TypeScript (missing from JS loop?)

  ### AgentFactory.ts {#src-agents-agentfactory.ts}
    # 🔗 TypeScript (missing from JS loop?)

  ### base-agent.js {#src-agents-base-agent.js}
    # ⚡ JavaScript/Node.js

  ### planner-agent.ts {#src-agents-planner-agent.ts}
    # 🔗 TypeScript (missing from JS loop?)

  ### planner-ai.js {#src-agents-planner-ai.js}
    # ⚡ JavaScript/Node.js

  ### ScreenshotAgent.cjs {#src-agents-screenshotagent.cjs}
    # ⚡ JavaScript/Node.js

  📁 **src/commands/**
  ### BaseCommand.cjs {#src-commands-basecommand.cjs}
    # ⚡ JavaScript/Node.js

  ### CommandRegistry.cjs {#src-commands-commandregistry.cjs}
    # ⚡ JavaScript/Node.js

  ### README.md {#src-commands-readme.md}
    # 📖 Documentation

    📁 **src/commands/automation/**
    📁 **src/commands/core/**
    ### test-runner.cjs {#src-commands-core-test-runner.cjs}
      # ⚡ JavaScript/Node.js

    ### validation-test.cjs {#src-commands-core-validation-test.cjs}
      # ⚡ JavaScript/Node.js

      📁 **src/commands/core/agents/**
      ### agents.md {#src-commands-core-agents-agents.md}
        # 📖 Documentation

      ### AgentsCommand.cjs {#src-commands-core-agents-agentscommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-agents-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-agents-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-agents-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/browser/**
      ### BrowserCommand.cjs {#src-commands-core-browser-browsercommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-browser-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-browser-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-browser-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/browserjs/**
      ### BrowserJSCommand.cjs {#src-commands-core-browserjs-browserjscommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-browserjs-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-browserjs-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-browserjs-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/chat/**
      ### ChatCommand.cjs {#src-commands-core-chat-chatcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-chat-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-chat-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-chat-readme.md}
        # 📖 Documentation

        📁 **src/commands/core/chat/test/**
        ### ChatCommand.test.js {#src-commands-core-chat-test-chatcommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/clear/**
      ### ClearCommand.cjs {#src-commands-core-clear-clearcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-clear-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-clear-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-clear-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/createroom/**
      ### CreateRoomCommand.cjs {#src-commands-core-createroom-createroomcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-createroom-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-createroom-package.json}
        # 📋 Configuration/Data

        📁 **src/commands/core/createroom/test/**
        ### CreateRoomCommand.test.js {#src-commands-core-createroom-test-createroomcommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/cursor/**
      ### CursorCommand.cjs {#src-commands-core-cursor-cursorcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-cursor-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-cursor-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-cursor-readme.md}
        # 📖 Documentation

        📁 **src/commands/core/cursor/graphics/**
        ### GraphicsRenderer.js {#src-commands-core-cursor-graphics-graphicsrenderer.js}
          # ⚡ JavaScript/Node.js

        📁 **src/commands/core/cursor/test/**
        ### ContinuonPositioning.test.js {#src-commands-core-cursor-test-continuonpositioning.test.js}
          # ⚡ JavaScript/Node.js

        ### CursorCommand.test.js {#src-commands-core-cursor-test-cursorcommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/diagnostics/**
      ### DiagnosticsCommand.cjs {#src-commands-core-diagnostics-diagnosticscommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.cjs {#src-commands-core-diagnostics-index.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-diagnostics-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-diagnostics-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-diagnostics-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/docs/**
      ### DocsCommand.cjs {#src-commands-core-docs-docscommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-docs-index.server.js}
        # ⚡ JavaScript/Node.js

      ### README.md {#src-commands-core-docs-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/emotion/**
      ### EmotionCommand.cjs {#src-commands-core-emotion-emotioncommand.cjs}
        # ⚡ JavaScript/Node.js

      ### emotionConfigs.cjs {#src-commands-core-emotion-emotionconfigs.cjs}
        # ⚡ JavaScript/Node.js

      ### emotionConfigs.js {#src-commands-core-emotion-emotionconfigs.js}
        # ⚡ JavaScript/Node.js

      ### emotionDefinition.cjs {#src-commands-core-emotion-emotiondefinition.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-emotion-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-emotion-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-emotion-readme.md}
        # 📖 Documentation

        📁 **src/commands/core/emotion/test/**
        ### EmotionAnimationTests.test.js {#src-commands-core-emotion-test-emotionanimationtests.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/exec/**
      ### ExecCommand.cjs {#src-commands-core-exec-execcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-exec-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-exec-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-exec-readme.md}
        # 📖 Documentation

        📁 **src/commands/core/exec/test/**
        ### ExecCommand.test.js {#src-commands-core-exec-test-execcommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/fileSave/**
      ### FileSaveCommand.cjs {#src-commands-core-filesave-filesavecommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-filesave-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-filesave-package.json}
        # 📋 Configuration/Data

        📁 **src/commands/core/fileSave/test/**
        ### FileSaveCommand.test.js {#src-commands-core-filesave-test-filesavecommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/findUser/**
      ### FindUserCommand.cjs {#src-commands-core-finduser-findusercommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.cjs {#src-commands-core-finduser-index.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-finduser-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-finduser-package.json}
        # 📋 Configuration/Data

      📁 **src/commands/core/help/**
      ### help.md {#src-commands-core-help-help.md}
        # 📖 Documentation

      ### HelpCommand.cjs {#src-commands-core-help-helpcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### index.cjs {#src-commands-core-help-index.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-help-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-help-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-help-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/info/**
      ### index.server.js {#src-commands-core-info-index.server.js}
        # ⚡ JavaScript/Node.js

      ### InfoCommand.cjs {#src-commands-core-info-infocommand.cjs}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-info-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-info-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/input/**
      ### index.server.js {#src-commands-core-input-index.server.js}
        # ⚡ JavaScript/Node.js

      ### InputCommand.cjs {#src-commands-core-input-inputcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-input-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-input-readme.md}
        # 📖 Documentation

        📁 **src/commands/core/input/test/**
        ### InputCommand.test.js {#src-commands-core-input-test-inputcommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/joinroom/**
      ### index.server.js {#src-commands-core-joinroom-index.server.js}
        # ⚡ JavaScript/Node.js

      ### JoinRoomCommand.cjs {#src-commands-core-joinroom-joinroomcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-joinroom-package.json}
        # 📋 Configuration/Data

        📁 **src/commands/core/joinroom/test/**
        ### JoinRoomCommand.test.js {#src-commands-core-joinroom-test-joinroomcommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/listagents/**
      ### index.server.js {#src-commands-core-listagents-index.server.js}
        # ⚡ JavaScript/Node.js

      ### ListAgentsCommand.cjs {#src-commands-core-listagents-listagentscommand.cjs}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-listagents-package.json}
        # 📋 Configuration/Data

        📁 **src/commands/core/listagents/test/**
        ### ListAgentsCommand.test.js {#src-commands-core-listagents-test-listagentscommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/listrooms/**
      ### ListRoomsCommand.cjs {#src-commands-core-listrooms-listroomscommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/loadrooms/**
      ### index.server.js {#src-commands-core-loadrooms-index.server.js}
        # ⚡ JavaScript/Node.js

      ### LoadRoomsCommand.cjs {#src-commands-core-loadrooms-loadroomscommand.cjs}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-loadrooms-package.json}
        # 📋 Configuration/Data

        📁 **src/commands/core/loadrooms/test/**
        ### LoadRoomsCommand.test.js {#src-commands-core-loadrooms-test-loadroomscommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/macro/**
      ### index.server.js {#src-commands-core-macro-index.server.js}
        # ⚡ JavaScript/Node.js

      ### MacroCommand.cjs {#src-commands-core-macro-macrocommand.cjs}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-macro-package.json}
        # 📋 Configuration/Data

      📁 **src/commands/core/markread/**
      📁 **src/commands/core/move/**
      ### index.server.js {#src-commands-core-move-index.server.js}
        # ⚡ JavaScript/Node.js

      ### MoveCommand.cjs {#src-commands-core-move-movecommand.cjs}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-move-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-move-readme.md}
        # 📖 Documentation

      📁 **src/commands/core/preferences/**
      ### index.server.js {#src-commands-core-preferences-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-preferences-package.json}
        # 📋 Configuration/Data

      ### PreferencesCommand.cjs {#src-commands-core-preferences-preferencescommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/promisejs/**
      ### index.server.js {#src-commands-core-promisejs-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-promisejs-package.json}
        # 📋 Configuration/Data

      ### PromiseJSCommand.cjs {#src-commands-core-promisejs-promisejscommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/reload/**
      ### index.server.js {#src-commands-core-reload-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-reload-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-reload-readme.md}
        # 📖 Documentation

      ### ReloadCommand.cjs {#src-commands-core-reload-reloadcommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/restart/**
      ### index.server.js {#src-commands-core-restart-index.server.js}
        # ⚡ JavaScript/Node.js

      ### README.md {#src-commands-core-restart-readme.md}
        # 📖 Documentation

      ### RestartCommand.cjs {#src-commands-core-restart-restartcommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/savefile/**
      ### index.server.js {#src-commands-core-savefile-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-savefile-package.json}
        # 📋 Configuration/Data

      ### SaveFileCommand.cjs {#src-commands-core-savefile-savefilecommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/screenshot/**
      ### ContinuonAnimations.css {#src-commands-core-screenshot-continuonanimations.css}
        # 📄 File

      ### ContinuonAnimator.js {#src-commands-core-screenshot-continuonanimator.js}
        # ⚡ JavaScript/Node.js

      ### index.cjs {#src-commands-core-screenshot-index.cjs}
        # ⚡ JavaScript/Node.js

      ### index.client.js {#src-commands-core-screenshot-index.client.js}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-screenshot-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-screenshot-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-screenshot-readme.md}
        # 📖 Documentation

      ### ScreenshotCommand.cjs {#src-commands-core-screenshot-screenshotcommand.cjs}
        # ⚡ JavaScript/Node.js

      ### ScreenshotCommand.client.js {#src-commands-core-screenshot-screenshotcommand.client.js}
        # ⚡ JavaScript/Node.js

      ### ScreenshotUtils.js {#src-commands-core-screenshot-screenshotutils.js}
        # ⚡ JavaScript/Node.js

        📁 **src/commands/core/screenshot/test/**
        ### Dependencies.test.js {#src-commands-core-screenshot-test-dependencies.test.js}
          # ⚡ JavaScript/Node.js

        ### IntegrationTests.test.js {#src-commands-core-screenshot-test-integrationtests.test.js}
          # ⚡ JavaScript/Node.js

        ### PromiseBasedAPI.test.js {#src-commands-core-screenshot-test-promisebasedapi.test.js}
          # ⚡ JavaScript/Node.js

        ### ServerCommand.test.js {#src-commands-core-screenshot-test-servercommand.test.js}
          # ⚡ JavaScript/Node.js

        ### ServerIntegration.test.js {#src-commands-core-screenshot-test-serverintegration.test.js}
          # ⚡ JavaScript/Node.js

        ### ValidationTests.test.js {#src-commands-core-screenshot-test-validationtests.test.js}
          # ⚡ JavaScript/Node.js

          📁 **src/commands/core/screenshot/test/browser-scripts/**
          ### bus_file_save.js {#src-commands-core-screenshot-test-browser-scripts-bus_file_save.js}
            # ⚡ JavaScript/Node.js

          ### check_command_execution.js {#src-commands-core-screenshot-test-browser-scripts-check_command_execution.js}
            # ⚡ JavaScript/Node.js

          ### check_console_warnings.js {#src-commands-core-screenshot-test-browser-scripts-check_console_warnings.js}
            # ⚡ JavaScript/Node.js

          ### check_server_logs.js {#src-commands-core-screenshot-test-browser-scripts-check_server_logs.js}
            # ⚡ JavaScript/Node.js

          ### check_server_reboot_handling.js {#src-commands-core-screenshot-test-browser-scripts-check_server_reboot_handling.js}
            # ⚡ JavaScript/Node.js

          ### complete_version_capture.js {#src-commands-core-screenshot-test-browser-scripts-complete_version_capture.js}
            # ⚡ JavaScript/Node.js

          ### enhance_websocket_handler.js {#src-commands-core-screenshot-test-browser-scripts-enhance_websocket_handler.js}
            # ⚡ JavaScript/Node.js

          ### execute_script.py {#src-commands-core-screenshot-test-browser-scripts-execute_script.py}
            # 🐍 Python

          ### generic_file_saver.js {#src-commands-core-screenshot-test-browser-scripts-generic_file_saver.js}
            # ⚡ JavaScript/Node.js

          ### list_available_commands.js {#src-commands-core-screenshot-test-browser-scripts-list_available_commands.js}
            # ⚡ JavaScript/Node.js

          ### test_bus_file_command.js {#src-commands-core-screenshot-test-browser-scripts-test_bus_file_command.js}
            # ⚡ JavaScript/Node.js

          ### test_scale_settings.js {#src-commands-core-screenshot-test-browser-scripts-test_scale_settings.js}
            # ⚡ JavaScript/Node.js

          ### trigger_server_file_save.js {#src-commands-core-screenshot-test-browser-scripts-trigger_server_file_save.js}
            # ⚡ JavaScript/Node.js

          ### version_check.js {#src-commands-core-screenshot-test-browser-scripts-version_check.js}
            # ⚡ JavaScript/Node.js

          ### version_monitor.js {#src-commands-core-screenshot-test-browser-scripts-version_monitor.js}
            # ⚡ JavaScript/Node.js

      📁 **src/commands/core/sentinel/**
      ### index.server.js {#src-commands-core-sentinel-index.server.js}
        # ⚡ JavaScript/Node.js

      ### README.md {#src-commands-core-sentinel-readme.md}
        # 📖 Documentation

      ### SentinelCommand.cjs {#src-commands-core-sentinel-sentinelcommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/share/**
      ### index.cjs {#src-commands-core-share-index.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-share-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-share-package.json}
        # 📋 Configuration/Data

      ### ShareCommand.cjs {#src-commands-core-share-sharecommand.cjs} 🌐 **GLOBAL NETWORK**
        # ⚡ JavaScript/Node.js - **Universal sharing across continuum ecosystem**
        # 🔗 FOUND: Global sharing infrastructure with intelligent routing
        # 🏅 ENABLES: Global AI competition network via `continuum://adapter/` URLs
        # 🚀 FEATURES: Share with user, agent, AI systems, UI - ecosystem diversity

      📁 **src/commands/core/spawn/**
      ### SpawnCommand.cjs {#src-commands-core-spawn-spawncommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/test/**
      ### index.cjs {#src-commands-core-test-index.cjs}
        # ⚡ JavaScript/Node.js

      ### index.server.js {#src-commands-core-test-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-test-package.json}
        # 📋 Configuration/Data

      ### TestCommand.cjs {#src-commands-core-test-testcommand.cjs}
        # ⚡ JavaScript/Node.js

        📁 **src/commands/core/test/test/**
        ### ModularCommandTests.test.js {#src-commands-core-test-test-modularcommandtests.test.js}
          # ⚡ JavaScript/Node.js

        ### TestCommand.test.js {#src-commands-core-test-test-testcommand.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/type/**
      ### index.server.js {#src-commands-core-type-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-type-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-type-readme.md}
        # 📖 Documentation

      ### TypeCommand.cjs {#src-commands-core-type-typecommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/validatecode/**
      ### index.server.js {#src-commands-core-validatecode-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-validatecode-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-validatecode-readme.md}
        # 📖 Documentation

      ### ValidateCodeCommand.cjs {#src-commands-core-validatecode-validatecodecommand.cjs}
        # ⚡ JavaScript/Node.js

      📁 **src/commands/core/validatejs/**
      ### index.server.js {#src-commands-core-validatejs-index.server.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-commands-core-validatejs-package.json}
        # 📋 Configuration/Data

      ### README.md {#src-commands-core-validatejs-readme.md}
        # 📖 Documentation

      ### ValidateJSCommand.cjs {#src-commands-core-validatejs-validatejscommand.cjs}
        # ⚡ JavaScript/Node.js

        📁 **src/commands/core/validatejs/test/**
        ### ValidateJSCommand.test.cjs {#src-commands-core-validatejs-test-validatejscommand.test.cjs}
          # ⚡ JavaScript/Node.js

      📁 **src/commands/core/workspace/**
      ### index.server.js {#src-commands-core-workspace-index.server.js}
        # ⚡ JavaScript/Node.js

      ### README.md {#src-commands-core-workspace-readme.md}
        # 📖 Documentation

      ### WorkspaceCommand.cjs {#src-commands-core-workspace-workspacecommand.cjs}
        # ⚡ JavaScript/Node.js

    📁 **src/commands/test/**
    ### BaseCommand.test.cjs {#src-commands-test-basecommand.test.cjs}
      # ⚡ JavaScript/Node.js

  📁 **src/core/**
  ### Academy.cjs {#src-core-academy.cjs}
    # ⚡ JavaScript/Node.js

  ### AdversarialPair.cjs {#src-core-adversarialpair.cjs}
    # ⚡ JavaScript/Node.js

  ### AICapabilityMatcher.cjs {#src-core-aicapabilitymatcher.cjs}
    # ⚡ JavaScript/Node.js

  ### AIModel.cjs {#src-core-aimodel.cjs}
    # ⚡ JavaScript/Node.js

  ### BrowserLogger.cjs {#src-core-browserlogger.cjs}
    # ⚡ JavaScript/Node.js

  ### CommandDefinitions.cjs {#src-core-commanddefinitions.cjs}
    # ⚡ JavaScript/Node.js

  ### CommandProcessor.cjs {#src-core-commandprocessor.cjs}
    # ⚡ JavaScript/Node.js

  ### CommandTeacher.cjs {#src-core-commandteacher.cjs}
    # ⚡ JavaScript/Node.js

  ### continuum-core.cjs {#src-core-continuum-core.cjs}
    # ⚡ JavaScript/Node.js

  ### CostTracker.cjs {#src-core-costtracker.cjs}
    # ⚡ JavaScript/Node.js

  ### FineTuningDataGenerator.cjs {#src-core-finetuningdatagenerator.cjs}
    # ⚡ JavaScript/Node.js

  ### GameTrainer.cjs {#src-core-gametrainer.cjs}
    # ⚡ JavaScript/Node.js

  ### MessageQueue.cjs {#src-core-messagequeue.cjs}
    # ⚡ JavaScript/Node.js

  ### ModelCaliber.cjs {#src-core-modelcaliber.cjs}
    # ⚡ JavaScript/Node.js

  ### Persona.cjs {#src-core-persona.cjs}
    # ⚡ JavaScript/Node.js

  ### PersonaBootcamp.cjs {#src-core-personabootcamp.cjs}
    # ⚡ JavaScript/Node.js

  ### PersonaFactory.cjs {#src-core-personafactory.cjs}
    # ⚡ JavaScript/Node.js

  ### PersonaLibrary.cjs {#src-core-personalibrary.cjs}
    # ⚡ JavaScript/Node.js

  ### PersonaRegistry.cjs {#src-core-personaregistry.cjs}
    # ⚡ JavaScript/Node.js

  ### ProtocolSheriff.cjs {#src-core-protocolsheriff.cjs}
    # ⚡ JavaScript/Node.js

  ### RequestManagerDroid.cjs {#src-core-requestmanagerdroid.cjs}
    # ⚡ JavaScript/Node.js

  ### SheriffTrainer.cjs {#src-core-sherifftrainer.cjs}
    # ⚡ JavaScript/Node.js

  ### TestingDroid.cjs {#src-core-testingdroid.cjs}
    # ⚡ JavaScript/Node.js

  ### ValidationPipeline.cjs {#src-core-validationpipeline.cjs}
    # ⚡ JavaScript/Node.js

  ### VersionManager.cjs {#src-core-versionmanager.cjs}
    # ⚡ JavaScript/Node.js

    📁 **src/core/academy/**
    ### README.md {#src-core-academy-readme.md}
      # 📖 Documentation

  📁 **src/docs/**
  ### COMMANDS.md {#src-docs-commands.md}
    # 📖 Documentation

  ### COMPLETION-SUMMARY.md {#src-docs-completion-summary.md}
    # 📖 Documentation

  ### GRACEFUL-SHUTDOWN.md {#src-docs-graceful-shutdown.md}
    # 📖 Documentation

  ### PROTOCOL.md {#src-docs-protocol.md}
    # 📖 Documentation

  📁 **src/integrations/**
  ### ContinuonRing.cjs {#src-integrations-continuonring.cjs}
    # ⚡ JavaScript/Node.js

  ### ContinuonTray.cjs {#src-integrations-continuontray.cjs}
    # ⚡ JavaScript/Node.js

  ### github-ci.cjs {#src-integrations-github-ci.cjs}
    # ⚡ JavaScript/Node.js

  ### HttpServer.cjs {#src-integrations-httpserver.cjs}
    # ⚡ JavaScript/Node.js

  ### MacOSMenuBar.cjs {#src-integrations-macosmenubar.cjs}
    # ⚡ JavaScript/Node.js

  ### ScreenshotIntegration.cjs {#src-integrations-screenshotintegration.cjs}
    # ⚡ JavaScript/Node.js

  ### SimpleMenuBar.cjs {#src-integrations-simplemenubar.cjs}
    # ⚡ JavaScript/Node.js

  ### SystemTray.cjs {#src-integrations-systemtray.cjs}
    # ⚡ JavaScript/Node.js

  ### SystemTraySimple.cjs {#src-integrations-systemtraysimple.cjs}
    # ⚡ JavaScript/Node.js

  ### WebSocketServer.cjs {#src-integrations-websocketserver.cjs}
    # ⚡ JavaScript/Node.js

  📁 **src/interfaces/**
  ### agent-interface.js {#src-interfaces-agent-interface.js}
    # ⚡ JavaScript/Node.js

  ### agent.interface.ts {#src-interfaces-agent.interface.ts}
    # 🔗 TypeScript (missing from JS loop?)

  ### tool-interface.js {#src-interfaces-tool-interface.js}
    # ⚡ JavaScript/Node.js

  📁 **src/modules/**
  ### CommandModule.cjs {#src-modules-commandmodule.cjs}
    # ⚡ JavaScript/Node.js

  ### CoreModule.cjs {#src-modules-coremodule.cjs}
    # ⚡ JavaScript/Node.js

  ### FluentAPI.cjs {#src-modules-fluentapi.cjs}
    # ⚡ JavaScript/Node.js

    📁 **src/modules/academy/**
    📁 **src/modules/agents/**
    📁 **src/modules/communication/**
    📁 **src/modules/ui/**
    ### ScreenshotFeedback.js {#src-modules-ui-screenshotfeedback.js}
      # ⚡ JavaScript/Node.js

    📁 **src/modules/validation/**
  📁 **src/services/**
  ### CommandDiscoveryService.cjs {#src-services-commanddiscoveryservice.cjs}
    # ⚡ JavaScript/Node.js

  ### GameManager.cjs {#src-services-gamemanager.cjs}
    # ⚡ JavaScript/Node.js

  ### ModelDiscoveryService.js {#src-services-modeldiscoveryservice.js}
    # ⚡ JavaScript/Node.js

  ### ModelDiscoveryService.ts {#src-services-modeldiscoveryservice.ts}
    # 🔗 TypeScript (missing from JS loop?)

  ### RemoteAgentManager.cjs {#src-services-remoteagentmanager.cjs}
    # ⚡ JavaScript/Node.js

  ### ScreenshotService.cjs {#src-services-screenshotservice.cjs}
    # ⚡ JavaScript/Node.js

  ### TabManager.cjs {#src-services-tabmanager.cjs}
    # ⚡ JavaScript/Node.js

  ### VisualGameManager.cjs {#src-services-visualgamemanager.cjs}
    # ⚡ JavaScript/Node.js

  ### WebVisualManager.cjs {#src-services-webvisualmanager.cjs}
    # ⚡ JavaScript/Node.js

  📁 **src/storage/**
  ### ModelCheckpoint.cjs {#src-storage-modelcheckpoint.cjs}
    # ⚡ JavaScript/Node.js

  ### PersistentStorage.cjs {#src-storage-persistentstorage.cjs}
    # ⚡ JavaScript/Node.js

    📁 **src/storage/persistent/**
    ### index.server.js {#src-storage-persistent-index.server.js}
      # ⚡ JavaScript/Node.js

    ### package.json {#src-storage-persistent-package.json}
      # 📋 Configuration/Data

    ### PersistentStorage.cjs {#src-storage-persistent-persistentstorage.cjs}
      # ⚡ JavaScript/Node.js

      📁 **src/storage/persistent/temp/**
      📁 **src/storage/persistent/test/**
      ### CleanStorage.test.js {#src-storage-persistent-test-cleanstorage.test.js}
        # ⚡ JavaScript/Node.js

      ### PersistentStorage.test.js {#src-storage-persistent-test-persistentstorage.test.js}
        # ⚡ JavaScript/Node.js

      ### SimpleStorage.test.js {#src-storage-persistent-test-simplestorage.test.js}
        # ⚡ JavaScript/Node.js

  📁 **src/tests/**
  ### ai-system.test.cjs {#src-tests-ai-system.test.cjs}
    # ⚡ JavaScript/Node.js

  ### continuum.test.cjs {#src-tests-continuum.test.cjs}
    # ⚡ JavaScript/Node.js

  ### demo-graceful-shutdown.cjs {#src-tests-demo-graceful-shutdown.cjs}
    # ⚡ JavaScript/Node.js

  ### orchestration.test.cjs {#src-tests-orchestration.test.cjs}
    # ⚡ JavaScript/Node.js

  ### run-all-tests.cjs {#src-tests-run-all-tests.cjs}
    # ⚡ JavaScript/Node.js

  ### self-awareness.test.cjs {#src-tests-self-awareness.test.cjs}
    # ⚡ JavaScript/Node.js

  ### status-indicator.test.cjs {#src-tests-status-indicator.test.cjs}
    # ⚡ JavaScript/Node.js

  ### test-agent-channels.cjs {#src-tests-test-agent-channels.cjs}
    # ⚡ JavaScript/Node.js

  ### test-ai-basic-tasks.cjs {#src-tests-test-ai-basic-tasks.cjs}
    # ⚡ JavaScript/Node.js

  ### test-ai-file-operations.cjs {#src-tests-test-ai-file-operations.cjs}
    # ⚡ JavaScript/Node.js

  ### test-ai-greeting.cjs {#src-tests-test-ai-greeting.cjs}
    # ⚡ JavaScript/Node.js

  ### test-ai-iterative.cjs {#src-tests-test-ai-iterative.cjs}
    # ⚡ JavaScript/Node.js

  ### test-ai-verifiable.cjs {#src-tests-test-ai-verifiable.cjs}
    # ⚡ JavaScript/Node.js

  ### test-ai-with-tools.cjs {#src-tests-test-ai-with-tools.cjs}
    # ⚡ JavaScript/Node.js

  ### test-continuum-spawn.cjs {#src-tests-test-continuum-spawn.cjs}
    # ⚡ JavaScript/Node.js

  ### test-continuum-system.cjs {#src-tests-test-continuum-system.cjs}
    # ⚡ JavaScript/Node.js

  ### test-continuum-web.cjs {#src-tests-test-continuum-web.cjs}
    # ⚡ JavaScript/Node.js

  ### test-coordination.test.cjs {#src-tests-test-coordination.test.cjs}
    # ⚡ JavaScript/Node.js

  ### test-everything.cjs {#src-tests-test-everything.cjs}
    # ⚡ JavaScript/Node.js

  ### test-graceful-shutdown.cjs {#src-tests-test-graceful-shutdown.cjs}
    # ⚡ JavaScript/Node.js

  ### test-interactive.cjs {#src-tests-test-interactive.cjs}
    # ⚡ JavaScript/Node.js

  ### test-live-continuum.cjs {#src-tests-test-live-continuum.cjs}
    # ⚡ JavaScript/Node.js

  ### test-real-ai-intelligence.cjs {#src-tests-test-real-ai-intelligence.cjs}
    # ⚡ JavaScript/Node.js

  ### test-real-interaction.cjs {#src-tests-test-real-interaction.cjs}
    # ⚡ JavaScript/Node.js

  ### test-real-pool.cjs {#src-tests-test-real-pool.cjs}
    # ⚡ JavaScript/Node.js

  ### test-routing-logic.cjs {#src-tests-test-routing-logic.cjs}
    # ⚡ JavaScript/Node.js

  ### test-send-function.cjs {#src-tests-test-send-function.cjs}
    # ⚡ JavaScript/Node.js

  ### test-shell-call.cjs {#src-tests-test-shell-call.cjs}
    # ⚡ JavaScript/Node.js

  ### test-simple-ai.cjs {#src-tests-test-simple-ai.cjs}
    # ⚡ JavaScript/Node.js

  ### test-smart-integration.cjs {#src-tests-test-smart-integration.cjs}
    # ⚡ JavaScript/Node.js

  ### test-tool-execution.cjs {#src-tests-test-tool-execution.cjs}
    # ⚡ JavaScript/Node.js

  ### test-working-pool.cjs {#src-tests-test-working-pool.cjs}
    # ⚡ JavaScript/Node.js

  📁 **src/tools/**
  ### filesystem-tool.ts {#src-tools-filesystem-tool.ts}
    # 🔗 TypeScript (missing from JS loop?)

  ### git-tool.ts {#src-tools-git-tool.ts}
    # 🔗 TypeScript (missing from JS loop?)

  ### PromiseJSExecutor.cjs {#src-tools-promisejsexecutor.cjs}
    # ⚡ JavaScript/Node.js

  ### web-fetch-tool.js {#src-tools-web-fetch-tool.js}
    # ⚡ JavaScript/Node.js

  ### web-fetch-tool.ts {#src-tools-web-fetch-tool.ts}
    # 🔗 TypeScript (missing from JS loop?)

  📁 **src/ui/**
  ### AcademyWebInterface.cjs {#src-ui-academywebinterface.cjs}
    # ⚡ JavaScript/Node.js

  ### command-handler.js {#src-ui-command-handler.js}
    # ⚡ JavaScript/Node.js

  ### continuum-api.js {#src-ui-continuum-api.js}
    # ⚡ JavaScript/Node.js

  ### ui-config.json {#src-ui-ui-config.json}
    # 📋 Configuration/Data

  ### UIGenerator.cjs {#src-ui-uigenerator.cjs}
    # ⚡ JavaScript/Node.js

  ### WebComponentsIntegration.cjs {#src-ui-webcomponentsintegration.cjs}
    # ⚡ JavaScript/Node.js

    📁 **src/ui/components/**
    ### AcademySection.js {#src-ui-components-academysection.js}
      # ⚡ JavaScript/Node.js

    ### AIWidget.js {#src-ui-components-aiwidget.js}
      # ⚡ JavaScript/Node.js

    ### ChatArea.js {#src-ui-components-chatarea.js}
      # ⚡ JavaScript/Node.js

    ### ChatHeader.js {#src-ui-components-chatheader.js}
      # ⚡ JavaScript/Node.js

    ### GlassMenu.js {#src-ui-components-glassmenu.js}
      # ⚡ JavaScript/Node.js

    ### RoomTabs.js {#src-ui-components-roomtabs.js}
      # ⚡ JavaScript/Node.js

    ### StatusPill.js {#src-ui-components-statuspill.js}
      # ⚡ JavaScript/Node.js

    ### UserDrawer.js {#src-ui-components-userdrawer.js}
      # ⚡ JavaScript/Node.js

      📁 **src/ui/components/ActiveProjects/**
      ### ActiveProjects.js {#src-ui-components-activeprojects-activeprojects.js}
        # ⚡ JavaScript/Node.js

      ### index.js {#src-ui-components-activeprojects-index.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-ui-components-activeprojects-package.json}
        # 📋 Configuration/Data

        📁 **src/ui/components/ActiveProjects/test/**
        ### ActiveProjects.simple.test.js {#src-ui-components-activeprojects-test-activeprojects.simple.test.js}
          # ⚡ JavaScript/Node.js

        ### ActiveProjects.widget.test.js {#src-ui-components-activeprojects-test-activeprojects.widget.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/ui/components/SavedPersonas/**
      ### index.js {#src-ui-components-savedpersonas-index.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-ui-components-savedpersonas-package.json}
        # 📋 Configuration/Data

      ### SavedPersonas.css {#src-ui-components-savedpersonas-savedpersonas.css}
        # 📄 File

      ### SavedPersonas.js {#src-ui-components-savedpersonas-savedpersonas.js}
        # ⚡ JavaScript/Node.js

        📁 **src/ui/components/SavedPersonas/test/**
        ### SavedPersonas.integration.test.js {#src-ui-components-savedpersonas-test-savedpersonas.integration.test.js}
          # ⚡ JavaScript/Node.js

        ### SavedPersonas.simple.test.js {#src-ui-components-savedpersonas-test-savedpersonas.simple.test.js}
          # ⚡ JavaScript/Node.js

        ### SavedPersonas.widget.test.js {#src-ui-components-savedpersonas-test-savedpersonas.widget.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/ui/components/shared/**
      ### BaseWidget.js {#src-ui-components-shared-basewidget.js}
        # ⚡ JavaScript/Node.js

      ### BaseWidget.test.js {#src-ui-components-shared-basewidget.test.js}
        # ⚡ JavaScript/Node.js

      ### SidebarWidget.js {#src-ui-components-shared-sidebarwidget.js}
        # ⚡ JavaScript/Node.js

      📁 **src/ui/components/UserSelector/**
      ### index.js {#src-ui-components-userselector-index.js}
        # ⚡ JavaScript/Node.js

      ### package.json {#src-ui-components-userselector-package.json}
        # 📋 Configuration/Data

      ### UserSelector.js {#src-ui-components-userselector-userselector.js}
        # ⚡ JavaScript/Node.js

      ### UserSelectorUtils.js {#src-ui-components-userselector-userselectorutils.js}
        # ⚡ JavaScript/Node.js

        📁 **src/ui/components/UserSelector/test/**
        ### UserSelector.screenshot.test.js {#src-ui-components-userselector-test-userselector.screenshot.test.js}
          # ⚡ JavaScript/Node.js

        ### UserSelector.simple.test.js {#src-ui-components-userselector-test-userselector.simple.test.js}
          # ⚡ JavaScript/Node.js

        ### UserSelector.widget.test.js {#src-ui-components-userselector-test-userselector.widget.test.js}
          # ⚡ JavaScript/Node.js

      📁 **src/ui/components/VersionWidget/**
        📁 **src/ui/components/VersionWidget/test/**
        ### VersionWidget.test.js {#src-ui-components-versionwidget-test-versionwidget.test.js}
          # ⚡ JavaScript/Node.js

    📁 **src/ui/styles/**
    📁 **src/ui/utils/**
    ### ComponentLoader.js {#src-ui-utils-componentloader.js}
      # ⚡ JavaScript/Node.js

    📁 **src/ui/widgets/**
    ### AgentWidget.js {#src-ui-widgets-agentwidget.js}
      # ⚡ JavaScript/Node.js

    ### BaseConnectionWidget.js {#src-ui-widgets-baseconnectionwidget.js}
      # ⚡ JavaScript/Node.js

    ### UnifiedSlideoutPanel.js {#src-ui-widgets-unifiedslideoutpanel.js}
      # ⚡ JavaScript/Node.js


📁 **templates/**
  📁 **templates/continuum-structure/**
  ### config.env {#templates-continuum-structure-config.env}
    # 📄 File

  ### README.md {#templates-continuum-structure-readme.md}
    # 📖 Documentation

    📁 **templates/continuum-structure/shared/**
    ### models.json {#templates-continuum-structure-shared-models.json}
      # 📋 Configuration/Data

    📁 **templates/continuum-structure/users/**
      📁 **templates/continuum-structure/users/EXAMPLE_USER/**
      ### config.env {#templates-continuum-structure-users-example_user-config.env}
        # 📄 File

  📁 **templates/enterprise/**
  ### config.json {#templates-enterprise-config.json}
    # 📋 Configuration/Data

  ### README.md {#templates-enterprise-readme.md}
    # 📖 Documentation

  📁 **templates/open-source/**
  ### config.json {#templates-open-source-config.json}
    # 📋 Configuration/Data

  ### README.md {#templates-open-source-readme.md}
    # 📖 Documentation

  📁 **templates/rapid-prototyping/**
  ### config.json {#templates-rapid-prototyping-config.json}
    # 📋 Configuration/Data

  ### README.md {#templates-rapid-prototyping-readme.md}
    # 📖 Documentation

  📁 **templates/standard/**
  ### config.json {#templates-standard-config.json}
    # 📋 Configuration/Data

  📁 **templates/tdd/**
  ### config.json {#templates-tdd-config.json}
    # 📋 Configuration/Data

  ### README.md {#templates-tdd-readme.md}
    # 📖 Documentation



---

## 🎯 Agent Study Guide - Archaeological Discoveries

### 🔍 **CONTINUUM SYSTEM ANALYSIS - DISCOVERY AUDIT COMPLETE**

Based on comprehensive archaeological investigation of git history, tombstones, and active system analysis, Continuum reveals itself as a **cutting-edge AI Workforce Construction Platform** with sophisticated capabilities that were systematically dismantled but remain recoverable.

### 📊 **Current System Status**
- **Active Commands**: 35 working commands via robust Command Bus Architecture
- **Core Automation**: `trust_the_process.py` - Complete 6-step development automation with integrity checking
- **Architecture**: Clean command bus with thin client adapters, Promise-based APIs
- **WebSocket Integration**: Real-time browser control and screenshot automation

### 🏛️ **Lost Treasures - Recoverable from Git History**

#### 🎓 **Continuum Academy v0.2.0** (Commit: f0e2fb9)
**Matrix-inspired adversarial AI training system**

**Core Components**:
- **Academy.cjs** - Adversarial training system with boot camp enrollment
- **TestingDroid.cjs** - GAN-style adversarial test generation challenging ProtocolSheriff
- **LoRAAdapter.cjs** - **190,735x storage reduction** with specialized fine-tuning
- **PersonaFactory.cjs** - AI persona creation and management
- **ModelCheckpoint.cjs** - Training state persistence

**Training Architecture**:
```javascript
// TestingDroid generates adversarial cases
TestingDroid vs ProtocolSheriff
// Academy manages boot camp graduation
Academy.enrollRecruit() -> bootCamp() -> graduation()
// LoRA saves only adapter weights (8-32 rank, 16 alpha scaling)
LoRA: 190,735 parameters vs 175B base model = 0.000109% storage
```

#### 🎮 **Mass Effect-Style UI System** (Commits: 4ffb32e, 41c02a2)
**Cyberpunk slideout panels with sophisticated agent selection**

**UI Components**:
- **AgentSelector.js** - Multi-agent selection with gradients, avatars, status indicators
- **SimpleAgentSelector.js** - Slideout panels with cyberpunk aesthetics
- **Glass morphism styling**: `rgba(0, 255, 136, 0.15)` with backdrop blur
- **Angular video game aesthetics** with clip-path polygons
- **Smooth animations**: `cubic-bezier(0.4, 0, 0.2, 1)`

**Design Principles**:
```css
/* Recovered from git history */
background: rgba(0, 255, 136, 0.15);
backdrop-filter: blur(15px);
clip-path: polygon(0 0, calc(100% - 20px) 0, 100% 20px, 100% 100%, 0 100%);
transition: transform 0.4s cubic-bezier(0.4, 0, 0.2, 1);
```

#### 🤖 **Intelligent Routing System** (Commit: 72c5684)
**Self-improving router with process management**

**Lost Components**:
- **intelligent-routing.cjs** - Smart agent selection and routing
- **process-manager.cjs** - Process lifecycle management
- **self-improving-router.cjs** - Learning-based routing optimization
- **tmux-claude-pool.cjs** - Multi-agent session management

### 🚀 **Active Automation Capabilities**

#### **trust_the_process.py** - 6-Step Development Cycle
**Currently functional and sophisticated**:

1. **Clear old data** - Screenshot cleanup to avoid confusion
2. **Make small change** - Surgical precision development 
3. **Bump version** - Auto-increment tracking
4. **Test immediately** - Screenshot + console + WebSocket validation
5. **Fix ANY errors** - Zero tolerance for breaking system
6. **Commit when stable** - Only when all criteria pass

**Real-time Validation**:
```python
success_criteria = {
    'agent_validation': True,
    'screenshot_capture': True, 
    'no_console_errors': True,
    'version_check': True,
    'websocket_connection': True
}
```

#### **Command Bus Architecture** - Currently Active
**35 sophisticated commands including**:

- **SCREENSHOT** - Browser canvas-based capture with WebSocket streaming
- **BROWSER_JS** - Base64-encoded JavaScript execution with real-time feedback
- **WORKSPACE** - Configurable path management (no hardcoded paths)
- **AGENTS** - Agent registration and management
- **BROWSER** - WebSocket-coordinated browser operations

### 🏗️ **System Architecture - Clean Design Patterns**

#### **Command Bus Pattern**
```
┌─────────────────────────────────────────┐
│           Continuum Server              │
│         (OS/Orchestrator)               │
│  ┌─────────────────────────────────────┐ │
│  │         Command Bus                 │ │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐  │ │
│  │  │work │ │restart│screenshot│browser│ │ 
│  │  │space│ │     │ │     │ │_js  │  │ │
│  │  └─────┘ └─────┘ └─────┘ └─────┘  │ │
│  └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
         ↑                    ↑
   ┌─────────┐          ┌─────────┐
   │ Python  │          │Browser  │
   │ Client  │          │ Client  │
   │(AI Portal)│        │   (UI)  │
   └─────────┘          └─────────┘
```

**Design Principles**:
- **Adapter Pattern**: Thin clients forward commands to server bus
- **Promise-Based**: Async/await patterns across all interfaces
- **Self-Documenting**: Help system generates live documentation
- **No God Objects**: Clean separation of concerns

### 🛠️ **Restoration Strategy**

#### **Phase 1: UI Restoration** (High Impact)
```bash
# Recover Mass Effect-style agent selectors
git show 4ffb32e:src/ui/components/AgentSelector.js > src/ui/components/AgentSelector.js
git show 41c02a2:src/ui/components/SimpleAgentSelector.js > src/ui/components/SimpleAgentSelector.js

# Test with existing automation
python python-client/trust_the_process.py --screenshot
```

#### **Phase 2: Academy System** (Medium Impact)
```bash
# Recover Academy training system
git show f0e2fb9:src/core/Academy.cjs > src/core/Academy.cjs
git show f0e2fb9:src/core/TestingDroid.cjs > src/core/TestingDroid.cjs
git show f0e2fb9:src/core/LoRAAdapter.cjs > src/core/LoRAAdapter.cjs

# Integrate with command bus
# Add academy commands to CommandRegistry
```

#### **Phase 3: Intelligent Routing** (Low Impact)
```bash
# Recover routing intelligence
git show 72c5684:src/core/intelligent-routing.cjs > src/core/intelligent-routing.cjs
git show 72c5684:src/core/process-manager.cjs > src/core/process-manager.cjs
```

### 📈 **Recovery Priority Matrix**

| Component | Impact | Complexity | Status | Recovery Command |
|-----------|--------|------------|--------|------------------|
| **UI Components** | 🔥 High | 🟢 Low | Git recoverable | `git show 4ffb32e:path` |
| **Academy System** | 🔥 High | 🟡 Medium | Git recoverable | `git show f0e2fb9:path` |
| **Automation** | ✅ Active | ✅ Working | Currently functional | `trust_the_process.py` |
| **Command Bus** | ✅ Active | ✅ Working | 35 commands active | `CommandRegistry.cjs` |
| **Routing** | 🟡 Medium | 🟡 Medium | Git recoverable | `git show 72c5684:path` |

### 🎯 **Development Workflow - Ready to Use**

```bash
# 1. Take archaeological screenshot
python python-client/trust_the_process.py --screenshot

# 2. Recover specific capability
git show f0e2fb9:src/core/Academy.cjs > src/core/Academy.cjs

# 3. Validate integrity  
python python-client/trust_the_process.py

# 4. Commit when stable
git add . && git commit -m "Restore Academy system"
```

**Success Criteria**: All 5 validation checks must pass
- ✅ Agent validation  
- ✅ Screenshot capture
- ✅ No console errors
- ✅ Version check  
- ✅ WebSocket connection

---

## 🔧 Maintenance Commands

```bash
# Regenerate this file
./scripts/generate-files-tree.sh

# Add to dashboard sync
python3 python-client/ai-portal.py --cmd docs

# Find files that might be consolidatable
find . -name "*.py" -o -name "*.js" | grep -E "(util|helper|common)" 

# Find potential dead code
find . -name "*.py" -o -name "*.js" | xargs grep -l "TODO.*remove\|FIXME.*delete\|deprecated"
```

## 📊 Structure Metrics

- **Total files**: $(find . -type f ! -path '*/node_modules/*' ! -path '*/.git/*' | wc -l | tr -d ' ')
- **Directory depth**: $(find . -type d ! -path '*/node_modules/*' ! -path '*/.git/*' | awk -F/ '{print NF-1}' | sort -nr | head -1)
- **Python files**: $(find . -name "*.py" ! -path '*/node_modules/*' | wc -l | tr -d ' ')
- **JavaScript files**: $(find . -name "*.js" -o -name "*.cjs" | ! -path '*/node_modules/*' | wc -l | tr -d ' ')

---
*Generated: $(date)*  
*Script: `./scripts/generate-files-tree.sh`*
