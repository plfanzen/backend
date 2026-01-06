// SPDX-FileCopyrightText: 2026 Aaron Dewes
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// Splits a string into a vector of substrings, where each substring is either a quoted string or a non-whitespace string.
/// Also handles escaped quotes. Removes the quotes from the output, unless they are escaped. If they are escaped, the escape character is removed.
///
/// # Arguments
///
/// * `input` - A string slice that holds the input string to be split.
///
/// # Returns
///
/// A vector of strings, where each string is a substring of the input string.
///
/// # Examples
///
/// ```
/// use n5i::utils::split_with_quotes;
///
/// let input = r#"hello "world" 'how are you'"#;
/// let expected_output = vec!["hello".to_string(), "world".to_string(), "how are you".to_string()];
/// let output = split_with_quotes(input);
///
/// assert_eq!(output, expected_output);
/// ```
pub fn split_with_quotes(input: &str) -> Vec<String> {
    let mut output: Vec<String> = Vec::new();
    let mut current_string = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';
    let mut escaped = false;
    for c in input.chars() {
        if escaped {
            current_string.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' || c == '\'' {
            if in_quotes && c == quote_char {
                in_quotes = false;
                quote_char = ' ';
            } else if !in_quotes {
                in_quotes = true;
                quote_char = c;
            } else if in_quotes && c != quote_char {
                current_string.push(c);
                continue;
            }
            continue;
        }
        if c.is_whitespace() && !in_quotes {
            if !current_string.is_empty() {
                output.push(current_string);
                current_string = String::new();
            }
            continue;
        }
        current_string.push(c);
    }
    if !current_string.is_empty() {
        output.push(current_string);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_with_quotes() {
        let input = r#"hello "world" 'how are you'"#;
        let expected_output = vec![
            "hello".to_string(),
            "world".to_string(),
            "how are you".to_string(),
        ];
        let output = split_with_quotes(input);
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_split_with_quotes_escaped_quotes() {
        let input = r#"hello "world \"how are you\"" goodbye"#;
        let expected_output = vec![
            "hello".to_string(),
            "world \"how are you\"".to_string(),
            "goodbye".to_string(),
        ];
        let output = split_with_quotes(input);
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_split_with_quotes_empty() {
        let input = "";
        let expected_output: Vec<String> = Vec::new();
        let output = split_with_quotes(input);
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_split_with_quotes_single() {
        let input = "hello";
        let expected_output = vec!["hello".to_string()];
        let output = split_with_quotes(input);
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_split_with_quotes_multiple_spaces() {
        let input = "hello    world";
        let expected_output = vec!["hello".to_string(), "world".to_string()];
        let output = split_with_quotes(input);
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_split_with_quotes_nested_quotes() {
        let input = r#"hello "world 'how are you'" goodbye"#;
        let expected_output = vec![
            "hello".to_string(),
            "world 'how are you'".to_string(),
            "goodbye".to_string(),
        ];
        let output = split_with_quotes(input);
        assert_eq!(output, expected_output);
    }
}
