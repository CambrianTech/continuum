/**
 * Visual Game Manager
 * Handles AI games through visual interface (screenshots + mouse/keyboard)
 * AIs learn to play by seeing the screen and clicking, just like humans
 */

class VisualGameManager {
  constructor(commandProcessor) {
    this.commandProcessor = commandProcessor;
    this.activeVisualGames = new Map(); // gameId -> visual game state
    this.screenshotInterval = 1000; // Default interval between screenshots
    this.defaultResolution = 'low'; // low, med, high
    this.gameIntervals = new Map(); // gameId -> intervalId
  }

  /**
   * Start a visual game where AIs interact through screenshots and clicks
   */
  async startVisualGame(gameType, players, options = {}) {
    const gameId = `visual_game_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    const game = {
      id: gameId,
      type: gameType,
      players: players,
      currentPlayer: 0,
      gameState: 'active',
      screenshots: [],
      moves: [],
      startTime: Date.now(),
      options: {
        screenshotInterval: options.screenshotInterval || this.screenshotInterval,
        resolution: options.resolution || this.defaultResolution,
        autoPlay: options.autoPlay || false,
        academy: options.academy || false,
        speed: options.speed || 2000
      }
    };

    this.activeVisualGames.set(gameId, game);
    
    console.log(`🎮📸 Started visual ${gameType}: ${players.join(' vs ')}`);
    console.log(`📷 Screenshot interval: ${game.options.screenshotInterval}ms`);
    console.log(`🔍 Resolution: ${game.options.resolution}`);

    // Initialize the game interface
    await this.initializeGameInterface(gameId);
    
    // Start screenshot monitoring
    this.startScreenshotMonitoring(gameId);
    
    // Start AI auto-play if enabled
    if (game.options.autoPlay) {
      this.startVisualAutoPlay(gameId);
    }

    return game;
  }

  /**
   * Initialize the game interface (draw board, setup UI)
   */
  async initializeGameInterface(gameId) {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return;

    // Take initial screenshot
    const initialScreenshot = await this.takeGameScreenshot(gameId, 'game-start');
    game.screenshots.push(initialScreenshot);

    console.log(`📸 Initial game screenshot taken: ${initialScreenshot.filename}`);
  }

  /**
   * Start continuous screenshot monitoring
   */
  startScreenshotMonitoring(gameId) {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return;

    const intervalId = setInterval(async () => {
      const currentGame = this.activeVisualGames.get(gameId);
      if (!currentGame || currentGame.gameState === 'finished') {
        console.log(`📸 Stopping screenshot monitoring for game: ${gameId}`);
        clearInterval(intervalId);
        this.gameIntervals.delete(gameId);
        return;
      }

      // Take periodic screenshot
      const screenshot = await this.takeGameScreenshot(gameId, 'monitoring');
      currentGame.screenshots.push(screenshot);
      
      // Limit screenshot history (keep last 20)
      if (currentGame.screenshots.length > 20) {
        currentGame.screenshots = currentGame.screenshots.slice(-20);
      }

      console.log(`📸 Game monitoring screenshot: ${screenshot.filename}`);

    }, game.options.screenshotInterval);

    this.gameIntervals.set(gameId, intervalId);
    console.log(`📷 Started screenshot monitoring for game: ${gameId}`);
  }

  /**
   * Take a game screenshot with context
   */
  async takeGameScreenshot(gameId, context = 'game') {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return null;

    const timestamp = Date.now();
    const filename = `visual-game-${gameId}-${context}-${timestamp}`;
    
    try {
      // Use existing screenshot command with resolution
      await this.commandProcessor.takeScreenshot(`${game.options.resolution} ${filename}`);
      
      const screenshot = {
        filename: filename,
        timestamp: timestamp,
        context: context,
        gameId: gameId,
        currentPlayer: game.players[game.currentPlayer],
        resolution: game.options.resolution
      };

      return screenshot;
    } catch (error) {
      console.error(`❌ Failed to take game screenshot: ${error.message}`);
      return null;
    }
  }

  /**
   * Start visual auto-play (AIs analyze screenshots and make moves)
   */
  async startVisualAutoPlay(gameId) {
    const game = this.activeVisualGames.get(gameId);
    if (!game || !game.options.autoPlay) return;

    console.log(`🤖📸 Starting visual auto-play for game: ${gameId}`);

    const playVisualMove = async () => {
      const currentGame = this.activeVisualGames.get(gameId);
      if (!currentGame || currentGame.gameState === 'finished') {
        console.log(`🏁 Visual auto-play ended for game: ${gameId}`);
        return;
      }

      const currentPlayer = currentGame.players[currentGame.currentPlayer];
      
      if (currentPlayer && (currentPlayer.includes('AI') || currentPlayer.includes('Bot'))) {
        console.log(`🤖👁️ ${currentPlayer} analyzing game state...`);
        
        // Take screenshot for analysis
        const analysisScreenshot = await this.takeGameScreenshot(gameId, 'ai-analysis');
        
        // Generate visual move based on screenshot
        const move = await this.generateVisualMove(gameId, currentPlayer, analysisScreenshot);
        
        if (move) {
          console.log(`🤖🖱️ ${currentPlayer} making visual move: ${move.type} at (${move.x}, ${move.y})`);
          
          // Execute the visual move
          await this.executeVisualMove(gameId, move);
          
          // Record the move
          currentGame.moves.push({
            player: currentPlayer,
            move: move,
            timestamp: Date.now(),
            screenshot: analysisScreenshot
          });

          // Switch players
          currentGame.currentPlayer = (currentGame.currentPlayer + 1) % currentGame.players.length;
          
          // Continue auto-play after delay
          setTimeout(playVisualMove, currentGame.options.speed);
        } else {
          console.log(`❌ ${currentPlayer} couldn't determine a move`);
          setTimeout(playVisualMove, currentGame.options.speed);
        }
      }
    };

    // Start the visual auto-play loop
    setTimeout(playVisualMove, 1000);
  }

  /**
   * Generate a visual move based on screenshot analysis
   */
  async generateVisualMove(gameId, player, screenshot) {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return null;

    // For now, simulate basic AI decision making
    // In a real implementation, this would use CV or AI analysis of the screenshot
    
    if (game.type === 'tic-tac-toe') {
      return this.generateTicTacToeVisualMove(game, player);
    } else if (game.type === 'chess') {
      return this.generateChessVisualMove(game, player);
    }

    // Default: random click in game area
    return {
      type: 'click',
      x: 300 + Math.floor(Math.random() * 400), // Game area
      y: 200 + Math.floor(Math.random() * 400),
      button: 'left'
    };
  }

  /**
   * Generate visual tic-tac-toe move (click on board position)
   */
  generateTicTacToeVisualMove(game, player) {
    // Simulate board positions as screen coordinates
    const boardPositions = [
      { x: 300, y: 200 }, { x: 400, y: 200 }, { x: 500, y: 200 }, // Top row
      { x: 300, y: 300 }, { x: 400, y: 300 }, { x: 500, y: 300 }, // Middle row
      { x: 300, y: 400 }, { x: 400, y: 400 }, { x: 500, y: 400 }  // Bottom row
    ];

    // Pick a random available position (in real implementation, analyze screenshot)
    const position = Math.floor(Math.random() * 9);
    const coords = boardPositions[position];

    return {
      type: 'click',
      x: coords.x,
      y: coords.y,
      button: 'left',
      gamePosition: position + 1
    };
  }

  /**
   * Execute a visual move using Continuon
   */
  async executeVisualMove(gameId, move) {
    try {
      // Activate Continuon
      await this.commandProcessor.activateAICursor();
      
      // Move to position
      await this.commandProcessor.mouseMove(`${move.x} ${move.y} smooth`);
      
      // Click
      await this.commandProcessor.mouseClick(`${move.x} ${move.y} ${move.button}`);
      
      // Take screenshot after move
      await this.takeGameScreenshot(gameId, 'post-move');
      
      console.log(`✅ Visual move executed: ${move.type} at (${move.x}, ${move.y})`);
      
    } catch (error) {
      console.error(`❌ Failed to execute visual move: ${error.message}`);
    }
  }

  /**
   * Request high-resolution screenshot on demand
   */
  async requestHighResScreenshot(gameId, reason = 'analysis') {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return null;

    console.log(`📸📈 Taking high-res screenshot for: ${reason}`);
    
    const highResScreenshot = await this.takeGameScreenshot(gameId, `high-res-${reason}`);
    if (highResScreenshot) {
      // Temporarily change resolution
      const originalRes = game.options.resolution;
      game.options.resolution = 'high';
      
      const screenshot = await this.takeGameScreenshot(gameId, reason);
      
      // Restore original resolution
      game.options.resolution = originalRes;
      
      return screenshot;
    }
    
    return null;
  }

  /**
   * Analyze game progress from screenshots
   */
  analyzeGameProgress(gameId) {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return null;

    const recentScreenshots = game.screenshots.slice(-5);
    const totalMoves = game.moves.length;
    const gameTime = Date.now() - game.startTime;

    return {
      gameId: gameId,
      type: game.type,
      players: game.players,
      currentPlayer: game.players[game.currentPlayer],
      totalMoves: totalMoves,
      gameTimeMs: gameTime,
      screenshotCount: game.screenshots.length,
      recentScreenshots: recentScreenshots.map(s => s.filename),
      averageTimePerMove: totalMoves > 0 ? gameTime / totalMoves : 0
    };
  }

  /**
   * Get visual game status
   */
  getVisualGameStatus(gameId) {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return null;

    return {
      ...game,
      analysis: this.analyzeGameProgress(gameId)
    };
  }

  /**
   * End visual game
   */
  endVisualGame(gameId) {
    const game = this.activeVisualGames.get(gameId);
    if (!game) return false;

    // Stop screenshot monitoring
    const intervalId = this.gameIntervals.get(gameId);
    if (intervalId) {
      clearInterval(intervalId);
      this.gameIntervals.delete(gameId);
    }

    // Take final screenshot
    this.takeGameScreenshot(gameId, 'game-end');

    game.gameState = 'finished';
    game.endTime = Date.now();

    console.log(`🏁📸 Visual game ended: ${gameId}`);
    console.log(`📊 Total screenshots: ${game.screenshots.length}`);
    console.log(`🎯 Total moves: ${game.moves.length}`);

    return true;
  }

  /**
   * Set screenshot interval
   */
  setScreenshotInterval(interval) {
    this.screenshotInterval = Math.max(100, Math.min(10000, interval));
    console.log(`📷 Screenshot interval set to: ${this.screenshotInterval}ms`);
  }

  /**
   * Set default resolution
   */
  setDefaultResolution(resolution) {
    const validResolutions = ['low', 'med', 'high'];
    if (validResolutions.includes(resolution)) {
      this.defaultResolution = resolution;
      console.log(`🔍 Default resolution set to: ${resolution}`);
    }
  }
}

module.exports = VisualGameManager;