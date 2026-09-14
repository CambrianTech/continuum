//! Integer arithmetic expression evaluator (benchmark: hard-rs / expr_eval).
//!
//! Supports + - * / (integer division), parentheses, correct operator
//! precedence (* and / bind tighter than + and -), left-associativity,
//! and arbitrary whitespace. Unary +/- on numbers is also accepted.
//! No external crates.

/// Evaluate an integer arithmetic expression.
pub fn eval(expr: &str) -> i64 {
    let mut p = Parser::new(expr);
    let v = p.parse_expr();
    p.skip_ws();
    if !p.at_end() {
        panic!("unexpected input at offset {}", p.i);
    }
    v
}

struct Parser<'a> {
    chars: Vec<char>,
    i: usize,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Parser<'a> {
    fn new(expr: &str) -> Self {
        let chars = expr.chars().collect();
        Parser {
            chars,
            i: 0,
            _marker: std::marker::PhantomData,
        }
    }

    fn skip_ws(&mut self) {
        while self.i < self.chars.len() && self.chars[self.i].is_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        if self.at_end() { None } else { Some(self.chars[self.i]) }
    }

    fn at_end(&self) -> bool {
        self.i >= self.chars.len()
    }

    /// Parse an expression: a sequence of terms joined by + and -, left-associative.
    fn parse_expr(&mut self) -> i64 {
        let mut v = self.parse_term();
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.i += 1;
                    v += self.parse_term();
                }
                Some('-') => {
                    self.i += 1;
                    v -= self.parse_term();
                }
                _ => break,
            }
        }
        v
    }

    /// Parse a term: factors joined by * and / (integer division), left-associative.
    fn parse_term(&mut self) -> i64 {
        let mut v = self.parse_factor();
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.i += 1;
                    v *= self.parse_factor();
                }
                Some('/') => {
                    self.i += 1;
                    let r = self.parse_factor();
                    if r == 0 {
                        panic!("division by zero");
                    }
                    v /= r; // i64 truncating division, matches Rust integer semantics
                }
                _ => break,
            }
        }
        v
    }

    /// Parse a factor: an optional unary +/- applied to a primary (number or parenthesized expr).
    fn parse_factor(&mut self) -> i64 {
        self.skip_ws();
        match self.peek() {
            Some('+') => {
                self.i += 1;
                self.parse_factor()
            }
            Some('-') => {
                self.i += 1;
                -self.parse_factor()
            }
            _ => self.parse_primary(),
        }
    }

    /// Parse a primary: a (possibly multi-digit) integer, or a parenthesized expression.
    fn parse_primary(&mut self) -> i64 {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.i += 1;
                let v = self.parse_expr();
                self.skip_ws();
                if self.peek() != Some(')') {
                    panic!("expected ')' at offset {}", self.i);
                }
                self.i += 1;
                v
            }
            Some(c) if c.is_ascii_digit() => {
                let mut n: i64 = 0;
                while let Some(d @ '0'..='9') = self.peek() {
                    n = n * 10 + (d as i64 - '0' as i64);
                    self.i += 1;
                }
                n
            }
            _ => panic!("unexpected input at offset {}", self.i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn benchmark_cases() {
        assert_eq!(eval("3 + 4 * 2"), 11);
        assert_eq!(eval("(3 + 4) * 2"), 14);
        assert_eq!(eval("10 - 2 - 3"), 5);
        assert_eq!(eval("2 * (3 + (4 - 1))"), 12);
        assert_eq!(eval("100 / 5 / 2"), 10);
        assert_eq!(eval("  7  +  3 * 2 "), 13);
        assert_eq!(eval("2 * 3 + 4 * 5"), 26);
    }

    #[test]
    fn extra_cases() {
        assert_eq!(eval("0"), 0);
        assert_eq!(eval("(1)"), 1);
        assert_eq!(eval("((7))"), 7);
        assert_eq!(eval("-3 + 5"), 2);
        assert_eq!(eval("-(2 + 3 * 4)"), -14);
        assert_eq!(eval("2 * -3"), -6);
        assert_eq!(eval("8 / (1 + 1)"), 4);
        assert_eq!(eval("7 / 2"), 3); // integer division truncates toward zero
        assert_eq!("-7 / 2", -3);
        assert_eq!(eval("\n\t 1 + \r\n 2 * 3 \t "), 7);
        assert_eq!(eval("((((((9))))))"), 9);
    }
}

fn main() {
    let cases: [(&str, i64); 18] = [
        ("3 + 4 * 2", 11),
        ("(3 + 4) * 2", 14),
        ("10 - 2 - 3", 5),
        ("2 * (3 + (4 - 1))", 12),
        ("100 / 5 / 2", 10),
        ("  7  +  3 * 2 ", 13),
        ("2 * 3 + 4 * 5", 26),
        ("0", 0),
        ("(1)", 1),
        ("((7))", 7),
        ("-3 + 5", 2),
        ("-(2 + 3 * 4)", -14),
        ("2 * -3", -6),
        ("8 / (1 + 1)", 4),
        ("7 / 2", 3),
        ("-7 / 2", -3),
        ("\n\t 1 + \r\n 2 * 3 \t ", 7),
        ("((((((9))))))", 9),
    ];
    let mut failed = 0usize;
    for (expr, want) in cases {
        match eval(expr) {
            got if got == want => println!("ok   {:?} -> {}", expr.trim(), got),
            got => {
                println!("FAIL {:?}: expected {}, got {}", expr.trim(), want, got);
                failed += 1;
            }
        }
    }
    if failed > 0 {
        eprintln!("{}/{} checks FAILED", failed, cases.len());
        std::process::exit(1);
    }
    println!("all {} checks passed", cases.len());
}
