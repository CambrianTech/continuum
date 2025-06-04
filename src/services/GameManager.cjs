/**
 * Game Manager
 * Handles simple games between humans and AI through Continuum
 */

class GameManager {
  constructor() {
    this.activeGames = new Map(); // gameId -> gameState
    this.spectators = new Map(); // gameId -> Set of spectator connections
    this.gameSpeed = 2000; // Default milliseconds between AI moves
    this.gameTypes = {
      'tic-tac-toe': this.createTicTacToe.bind(this),
      'word-chain': this.createWordChain.bind(this),
      'riddle': this.createRiddle.bind(this),
      '20-questions': this.createTwentyQuestions.bind(this)
    };
  }

  /**
   * Start a new game
   */
  startGame(gameType, players, options = {}) {
    const gameId = `game_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    if (!this.gameTypes[gameType]) {
      throw new Error(`Unknown game type: ${gameType}`);
    }
    
    const game = this.gameTypes[gameType](gameId, players);
    game.options = {
      autoPlay: options.autoPlay || false,
      speed: options.speed || this.gameSpeed,
      academy: options.academy || false,
      spectatable: options.spectatable !== false
    };
    
    this.activeGames.set(gameId, game);
    this.spectators.set(gameId, new Set());
    
    console.log(`🎮 Started ${gameType} game: ${gameId}`);
    console.log(`👥 Players: ${players.join(' vs ')}`);
    
    // Start AI auto-play if enabled
    if (game.options.autoPlay) {
      this.startAutoPlay(gameId);
    }
    
    return game;
  }

  /**
   * Start auto-play for AI vs AI games
   */
  async startAutoPlay(gameId) {
    const game = this.activeGames.get(gameId);
    if (!game || !game.options.autoPlay) return;
    
    console.log(`🤖 Starting auto-play for game: ${gameId}`);
    
    const playNextMove = async () => {
      const currentGame = this.activeGames.get(gameId);
      if (!currentGame || currentGame.getStatus().gameOver) {
        console.log(`🏁 Auto-play ended for game: ${gameId}`);
        return;
      }
      
      const status = currentGame.getStatus();
      const currentPlayer = status.currentPlayer;
      
      if (currentPlayer && (currentPlayer.includes('AI') || currentPlayer.includes('Bot'))) {
        // Generate AI move
        const move = await this.generateAIMove(currentGame);
        if (move) {
          console.log(`🤖 ${currentPlayer} auto-move: ${move}`);
          const result = this.makeMove(gameId, currentPlayer, move);
          
          // Broadcast to spectators
          this.broadcastToSpectators(gameId, {
            type: 'gameMove',
            player: currentPlayer,
            move: move,
            result: result,
            gameState: currentGame.getStatus()
          });
          
          // Continue auto-play after delay
          if (!result.gameOver) {
            setTimeout(playNextMove, currentGame.options.speed);
          } else {
            console.log(`🏆 Auto-play game finished! Winner: ${result.winner || 'Tie'}`);
            this.broadcastToSpectators(gameId, {
              type: 'gameEnd',
              winner: result.winner,
              finalState: currentGame.getStatus()
            });
          }
        }
      }
    };
    
    // Start the auto-play loop
    setTimeout(playNextMove, 1000);
  }

  /**
   * Generate AI move based on game type
   */
  async generateAIMove(game) {
    if (game.type === 'tic-tac-toe') {
      return this.generateTicTacToeAIMove(game);
    } else if (game.type === 'word-chain') {
      return this.generateWordChainAIMove(game);
    }
    return null;
  }

  /**
   * Add spectator to a game
   */
  addSpectator(gameId, spectatorConnection) {
    if (!this.spectators.has(gameId)) {
      this.spectators.set(gameId, new Set());
    }
    
    this.spectators.get(gameId).add(spectatorConnection);
    console.log(`👁️ Spectator added to game: ${gameId}`);
    
    // Send current game state to new spectator
    const game = this.activeGames.get(gameId);
    if (game) {
      spectatorConnection.send(JSON.stringify({
        type: 'gameState',
        gameId: gameId,
        state: game.getStatus()
      }));
    }
  }

  /**
   * Remove spectator from a game
   */
  removeSpectator(gameId, spectatorConnection) {
    if (this.spectators.has(gameId)) {
      this.spectators.get(gameId).delete(spectatorConnection);
      console.log(`👁️ Spectator removed from game: ${gameId}`);
    }
  }

  /**
   * Broadcast message to all spectators of a game
   */
  broadcastToSpectators(gameId, message) {
    const gameSpectators = this.spectators.get(gameId);
    if (!gameSpectators) return;
    
    const messageStr = JSON.stringify({
      ...message,
      gameId: gameId,
      timestamp: new Date().toISOString()
    });
    
    gameSpectators.forEach(spectator => {
      try {
        spectator.send(messageStr);
      } catch (error) {
        console.error('Failed to send to spectator:', error);
        gameSpectators.delete(spectator);
      }
    });
  }

  /**
   * Set game speed for auto-play
   */
  setGameSpeed(speed) {
    this.gameSpeed = Math.max(100, Math.min(10000, speed)); // 100ms to 10s
    console.log(`⚡ Game speed set to: ${this.gameSpeed}ms`);
  }

  /**
   * Make a move in a game
   */
  makeMove(gameId, player, move) {
    const game = this.activeGames.get(gameId);
    if (!game) {
      throw new Error(`Game not found: ${gameId}`);
    }
    
    return game.makeMove(player, move);
  }

  /**
   * Get game status
   */
  getGameStatus(gameId) {
    const game = this.activeGames.get(gameId);
    return game ? game.getStatus() : null;
  }

  /**
   * Tic-Tac-Toe Game
   */
  createTicTacToe(gameId, players) {
    return {
      id: gameId,
      type: 'tic-tac-toe',
      players: players,
      currentPlayer: 0,
      board: Array(9).fill(null),
      winner: null,
      moves: 0,
      
      makeMove(player, position) {
        if (this.winner) {
          return { success: false, message: 'Game already finished!' };
        }
        
        if (this.players[this.currentPlayer] !== player) {
          return { success: false, message: 'Not your turn!' };
        }
        
        if (this.board[position] !== null) {
          return { success: false, message: 'Position already taken!' };
        }
        
        // Make the move
        this.board[position] = this.currentPlayer === 0 ? 'X' : 'O';
        this.moves++;
        
        // Check for winner
        const winner = this.checkWinner();
        if (winner) {
          this.winner = winner;
        }
        
        // Switch players
        this.currentPlayer = 1 - this.currentPlayer;
        
        return {
          success: true,
          board: this.formatBoard(),
          winner: this.winner,
          nextPlayer: this.winner ? null : this.players[this.currentPlayer],
          gameOver: this.winner !== null || this.moves === 9
        };
      },
      
      checkWinner() {
        const lines = [
          [0, 1, 2], [3, 4, 5], [6, 7, 8], // rows
          [0, 3, 6], [1, 4, 7], [2, 5, 8], // columns
          [0, 4, 8], [2, 4, 6] // diagonals
        ];
        
        for (const [a, b, c] of lines) {
          if (this.board[a] && this.board[a] === this.board[b] && this.board[a] === this.board[c]) {
            return this.players[this.board[a] === 'X' ? 0 : 1];
          }
        }
        
        return this.moves === 9 ? 'tie' : null;
      },
      
      formatBoard() {
        const display = this.board.map((cell, i) => cell || (i + 1)).join('');
        return `
🎯 Tic-Tac-Toe Board:
 ${display[0]} | ${display[1]} | ${display[2]} 
-----------
 ${display[3]} | ${display[4]} | ${display[5]} 
-----------
 ${display[6]} | ${display[7]} | ${display[8]} `;
      },
      
      getStatus() {
        return {
          id: this.id,
          type: this.type,
          players: this.players,
          currentPlayer: this.winner ? null : this.players[this.currentPlayer],
          board: this.formatBoard(),
          winner: this.winner,
          gameOver: this.winner !== null || this.moves === 9
        };
      }
    };
  }

  /**
   * Word Chain Game
   */
  createWordChain(gameId, players) {
    return {
      id: gameId,
      type: 'word-chain',
      players: players,
      currentPlayer: 0,
      words: [],
      winner: null,
      
      makeMove(player, word) {
        if (this.winner) {
          return { success: false, message: 'Game already finished!' };
        }
        
        if (this.players[this.currentPlayer] !== player) {
          return { success: false, message: 'Not your turn!' };
        }
        
        word = word.toLowerCase().trim();
        
        // Check if word follows the chain
        if (this.words.length > 0) {
          const lastWord = this.words[this.words.length - 1];
          if (word[0] !== lastWord[lastWord.length - 1]) {
            return { 
              success: false, 
              message: `Word must start with '${lastWord[lastWord.length - 1]}'!` 
            };
          }
        }
        
        // Check if word was already used
        if (this.words.includes(word)) {
          return { success: false, message: 'Word already used!' };
        }
        
        // Add word to chain
        this.words.push(word);
        this.currentPlayer = 1 - this.currentPlayer;
        
        return {
          success: true,
          word: word,
          chain: this.words.join(' → '),
          nextPlayer: this.players[this.currentPlayer],
          nextLetter: word[word.length - 1]
        };
      },
      
      getStatus() {
        return {
          id: this.id,
          type: this.type,
          players: this.players,
          currentPlayer: this.winner ? null : this.players[this.currentPlayer],
          chain: this.words.join(' → '),
          nextLetter: this.words.length > 0 ? this.words[this.words.length - 1].slice(-1) : null,
          wordsUsed: this.words.length
        };
      }
    };
  }

  /**
   * Get available games
   */
  getAvailableGames() {
    return Object.keys(this.gameTypes);
  }

  /**
   * List active games
   */
  getActiveGames() {
    return Array.from(this.activeGames.values()).map(game => game.getStatus());
  }

  /**
   * End a game
   */
  endGame(gameId) {
    const deleted = this.activeGames.delete(gameId);
    if (deleted) {
      console.log(`🏁 Ended game: ${gameId}`);
    }
    return deleted;
  }

  /**
   * Generate AI move for Tic-Tac-Toe
   */
  generateTicTacToeAIMove(game) {
    const board = game.board;
    const symbol = game.currentPlayer === 0 ? 'X' : 'O';
    const opponentSymbol = game.currentPlayer === 0 ? 'O' : 'X';
    
    // 1. Try to win
    for (let i = 0; i < 9; i++) {
      if (board[i] === null) {
        board[i] = symbol;
        if (this.checkTicTacToeWin(board, symbol)) {
          board[i] = null;
          return (i + 1).toString();
        }
        board[i] = null;
      }
    }
    
    // 2. Block opponent
    for (let i = 0; i < 9; i++) {
      if (board[i] === null) {
        board[i] = opponentSymbol;
        if (this.checkTicTacToeWin(board, opponentSymbol)) {
          board[i] = null;
          return (i + 1).toString();
        }
        board[i] = null;
      }
    }
    
    // 3. Take center
    if (board[4] === null) return '5';
    
    // 4. Take corners
    const corners = [0, 2, 6, 8];
    for (const corner of corners) {
      if (board[corner] === null) {
        return (corner + 1).toString();
      }
    }
    
    // 5. Take any available spot
    for (let i = 0; i < 9; i++) {
      if (board[i] === null) {
        return (i + 1).toString();
      }
    }
    
    return null;
  }

  /**
   * Check for Tic-Tac-Toe win
   */
  checkTicTacToeWin(board, symbol) {
    const lines = [
      [0, 1, 2], [3, 4, 5], [6, 7, 8], // rows
      [0, 3, 6], [1, 4, 7], [2, 5, 8], // columns
      [0, 4, 8], [2, 4, 6] // diagonals
    ];
    
    return lines.some(([a, b, c]) => 
      board[a] === symbol && board[b] === symbol && board[c] === symbol
    );
  }

  /**
   * Generate AI move for Word Chain
   */
  generateWordChainAIMove(game) {
    const words = game.words;
    const commonWords = [
      'apple', 'elephant', 'tiger', 'robot', 'table', 'energy', 'yellow', 'water',
      'radio', 'ocean', 'night', 'tree', 'eagle', 'lime', 'mouse', 'earth',
      'happy', 'yarn', 'nest', 'truck', 'kite', 'egg', 'green', 'nail', 'lamp',
      'pencil', 'lion', 'notebook', 'keyboard', 'dance', 'exercise', 'river'
    ];
    
    if (words.length === 0) {
      return commonWords[Math.floor(Math.random() * 8)]; // Pick from first 8
    }
    
    const lastWord = words[words.length - 1];
    const requiredLetter = lastWord[lastWord.length - 1];
    
    // Find valid words
    const validWords = commonWords.filter(word => 
      word[0] === requiredLetter && !words.includes(word)
    );
    
    if (validWords.length > 0) {
      return validWords[Math.floor(Math.random() * validWords.length)];
    }
    
    // Fallback
    return requiredLetter + 'at';
  }
}

module.exports = GameManager;