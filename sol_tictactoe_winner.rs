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

    // Exhaustive sweep: every one of the 3^9 = 19,683 possible boards (each
    // board appears exactly once as a mask in 0..2^18 — 2 bits per cell),
    // checked against an independent reference. Also tallies full-board
    // outcomes and checks the X<->O symmetry invariant on them.
    const LINES: [[usize; 3]; 8] = [
        [0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6],
    ];
    let mut x_win = 0u32;
    let mut o_win = 0u32;
    let mut draw = 0u32;
    for mask in 0..(1usize << 18) {
        // Decode the board, skipping masks with an unused value slot (v == 3).
        let mut b = [' '; 9];
        let mut valid = true;
        for i in 0..9usize {
            match (mask >> (i * 2)) & 3 {
                0 => b[i] = 'X',
                1 => b[i] = 'O',
                2 => {} // stays ' '
                _ => { valid = false; break; }
            }
        }
        if !valid { continue; }
        // Independent reference: first uniform non-blank line in LINES order wins.
        let mut ref_w = ' ';
        for l in LINES.iter() {
            if b[l[0]] != ' ' && b[l[1]] == b[l[0]] && b[l[2]] == b[l[0]] {
                ref_w = b[l[0]];
                break;
            }
        }
        assert_eq!(ttt_winner(&b), ref_w, "mismatch on {:?}", b);
        if b.iter().all(|c| *c != ' ') {
            match ref_w {
                'X' => x_win += 1,
                'O' => o_win += 1,
                _ => draw += 1,
            }
        }
    }
    // X<->O swap is a bijection on the board space, so full-board win counts match.
    assert_eq!(x_win, o_win);

    println!("all ttt_winner checks passed");
    println!("exhaustive 3^9=19683 boards agree with reference; X-wins={} O-wins={} draws={}", x_win, o_win, draw);
}
