fn main() {
    // Test cases
    println!("{}", eval("3 + 5")); // Should print 8
    println!("{}", eval("10 - 2 * 3")); // Should print 4
    println!("{}", eval("(10 + 2) * 3")); // Should print 36
    println!("{}", eval("10 / 2 + 3")); // Should print 8
}

pub fn eval(expr: &str) -> i64 {
    let mut tokens = tokenize(expr);
    parse(&mut tokens)
}

fn tokenize(expr: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '0'..='9' => {
                let mut num_str = String::new();
                while i < chars.len() && chars[i].is_digit(10) {
                    num_str.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token::Number(num_str.parse().unwrap()));
            },
            '+' | '-' | '*' | '/' => {
                tokens.push(Token::Operator(chars[i]));
                i += 1;
            },
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            },
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            },
            ' ' => {
                i += 1;
            },
            _ => panic!("Unexpected character: {}", chars[i]),
        }
    }

    tokens
}

#[derive(Debug)]
enum Token {
    Number(i64),
    Operator(char),
    LParen,
    RParen,
}

fn parse(tokens: &mut Vec<Token>) -> i64 {
    let mut result = parse_term(tokens);

    while !tokens.is_empty() && matches!(tokens[0], Token::Operator('+') | Token::Operator('-')) {
        match tokens.remove(0) {
            Token::Operator('+') => result += parse_term(tokens),
            Token::Operator('-') => result -= parse_term(tokens),
            _ => unreachable!(),
        }
    }

    result
}

fn parse_term(tokens: &mut Vec<Token>) -> i64 {
    let mut result = parse_factor(tokens);

    while !tokens.is_empty() && matches!(tokens[0], Token::Operator('*') | Token::Operator('/')) {
        match tokens.remove(0) {
            Token::Operator('*') => result *= parse_factor(tokens),
            Token::Operator('/') => result /= parse_factor(tokens),
            _ => unreachable!(),
        }
    }

    result
}

fn parse_factor(tokens: &mut Vec<Token>) -> i64 {
    if let Some(Token::Number(num)) = tokens.remove(0) {
        num
    } else if let Some(Token::LParen) = tokens.remove(0) {
        let result = parse(tokens);
        assert_eq!(tokens.remove(0), Token::RParen, "Expected closing parenthesis");
        result
    } else {
        panic!("Unexpected token: {:?}", tokens[0]);
    }
}