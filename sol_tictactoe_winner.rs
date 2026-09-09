//! Solution for benchmark task `tictactoe_winner` (suite games-rs).
//!
//! A tic-tac-toe board is given row-major as 9 cells, each 'X', 'O', or ' '.
//! This module reports the winner's mark ('X'/'O') when any of the three rows,
//! three columns, or two diagonals is a three-in-a-row for one player; otherwise
//! it reports no winner (' '). No external crates.

/// Return the winning mark if either player has three in a row (any row, column,
/// or diagonal), else ' '.
pub fn ttt_winner(b: &[char; 9]) -> char {
    const LINES: [[usize; 3]; 8] = [
        [0, 1, 2], // top row
        [3, 4, 5], // middle row
        [6, 7, 8], // bottom row
        [0, 3, 6], // left column
        [1, 4, 7], // center column
        [2, 5, 8], // right column
        [0, 4, 8], // main diagonal
        [2, 4, 6], // anti-diagonal
    ];

    for line in LINES.iter() {
        let (i, j, k) = (line[0], line[1], line[2]);
        let c = b[i];
        if c != ' ' && c == b[j] && c == b[k] {
            return c;
        }
    }

    ' '
}

/// Checks devised for verification: the three assertions shipped with the task,
/// plus edge cases (columns, anti-diagonal, both lines present → first wins).
fn main() {
    // Task-shipped checks.
    assert_eq!(ttt_winner(&['X', 'X', 'X', 'O', 'O', ' ', ' ', ' ', ' ']), 'X');
    assert_eq!(ttt_winner(&['X', 'O', 'X', 'O', 'X', 'O', 'O', 'X', 'O']), ' ');
    assert_eq!(ttt_winner(&['O', ' ', ' ', 'O', ' ', ' ', 'O', ' ', ' ']), 'O');

    // Additional checks.
    let mid_row = [' ', ' ', ' ', 'O', 'O', 'O', 'X', 'X', 'X']; // O row wins (checked before X's)
    assert_eq!(ttt_winner(&mid_row), 'O');

    let left_col = ['X', 'O', ' ', 'X', 'O', ' ', 'X', 'O', ' '];
    assert_eq!(ttt_winner(&left_col), 'X');

    let right_col = ['X', 'O', ' ', 'O', 'X', ' ', 'O', 'X', ' ']; // column 2: X, O, X → no line at all
    assert_eq!(ttt_winner(&right_col), ' ');

    let center_col = ['X', 'O', 'X', 'O', 'X', 'O', 'O', ' ', 'O']; // middle column (1,4,7) is O,X,O → no line at all
    assert_eq!(ttt_winner(&center_col), ' ');

    let anti_diag = [' ', ' ', 'O', ' ', 'O', ' ', 'O', ' ', ' '];
    assert_eq!(ttt_winner(&anti_diag), 'O');

    // Both a row and a diagonal for the same player: still that mark.
    let both = ['X', 'X', 'X', 'O', 'X', 'O', 'O', 'O', ' '];
    assert_eq!(ttt_winner(&both), 'X');

    // Empty board → no winner.
    let empty = [' '; 9];
    assert_eq!(ttt_winner(&empty), ' ');

    println!("all ttt_winner checks passed");
}
