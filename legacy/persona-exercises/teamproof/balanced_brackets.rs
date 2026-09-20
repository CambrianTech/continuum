use std::collections::VecDeque;

pub fn is_balanced(s: &str) -> bool {
    let mut stack: VecDeque<char> = VecDeque::new();
    let bracket_pairs = [(')', '('), (']', '['), ('}', '{')].iter().cloned().collect::<std::collections::HashMap<_, _>>();

    for c in s.chars() {
        match c {
            '(' | '[' | '{' => stack.push_back(c),
            ')' | ']' | '}' => {
                if let Some(open_bracket) = stack.pop_back() {
                    if bracket_pairs[&c] != open_bracket {
                        return false;
                    }
                } else {
                    return false;
                }
            },
            _ => {}
        }
    }

    stack.is_empty()
}

fn main() {
    let test_cases = vec![
        ("()", true),
        ("(())", true),
        ("(()())", true),
        ("{[()]}", true),
        ("{[(])}", false),
        ("(((", false),
        (")")", false),
        (""", true),
        ("abc", true),
    ];

    for (input, expected) in test_cases {
        assert_eq!(is_balanced(input), expected, "Test failed with input: {}", input);
    }

    println!("All tests passed!");
}