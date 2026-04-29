use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) enum Token {
    Literal(String),
    Placeholder(String),
}

/// A compiled pattern with pre-computed guards for fast rejection.
#[derive(Debug, Clone)]
pub(crate) struct CompiledPattern {
    pub tokens: Vec<Token>,
    pub min_len: usize,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub must_contain: Vec<String>,
}

impl CompiledPattern {
    /// Parse a template string like `foo-{bar}` or `/foo/{path}` into tokens.
    pub fn parse(template: &str) -> Self {
        let mut tokens = Vec::new();
        let mut rest = template;

        while !rest.is_empty() {
            if let Some(open) = rest.find('{') {
                if open > 0 {
                    tokens.push(Token::Literal(rest[..open].to_string()));
                }
                let close = rest[open..]
                    .find('}')
                    .expect("unmatched '{' in template")
                    + open;
                let name = &rest[open + 1..close];
                tokens.push(Token::Placeholder(name.to_string()));
                rest = &rest[close + 1..];
            } else {
                tokens.push(Token::Literal(rest.to_string()));
                break;
            }
        }

        let (min_len, prefix, suffix, must_contain) = Self::compute_guards(&tokens);

        Self {
            tokens,
            min_len,
            prefix,
            suffix,
            must_contain,
        }
    }

    fn compute_guards(tokens: &[Token]) -> (usize, Option<String>, Option<String>, Vec<String>) {
        // min_len = sum of all literal lengths
        let min_len: usize = tokens
            .iter()
            .filter_map(|t| match t {
                Token::Literal(s) => Some(s.len()),
                Token::Placeholder(_) => None,
            })
            .sum();

        // prefix: leading literal(s) before first placeholder
        let prefix = {
            let mut buf = String::new();
            for t in tokens {
                match t {
                    Token::Literal(s) => buf.push_str(s),
                    Token::Placeholder(_) => break,
                }
            }
            if buf.is_empty() { None } else { Some(buf) }
        };

        // suffix: trailing literal(s) after last placeholder
        let suffix = {
            let mut buf = String::new();
            for t in tokens.iter().rev() {
                match t {
                    Token::Literal(s) => buf.insert_str(0, s),
                    Token::Placeholder(_) => break,
                }
            }
            if buf.is_empty() { None } else { Some(buf) }
        };

        // must_contain: interior literal fragments between placeholders
        let mut must_contain = Vec::new();
        let mut seen_placeholder = false;
        let mut trailing_literals = Vec::new();
        for t in tokens {
            match t {
                Token::Literal(s) => {
                    if seen_placeholder {
                        trailing_literals.push(s.clone());
                    }
                }
                Token::Placeholder(_) => {
                    // Any literals accumulated after a previous placeholder
                    // but before this one are interior fragments.
                    if seen_placeholder && !trailing_literals.is_empty() {
                        must_contain.push(trailing_literals.join(""));
                        trailing_literals.clear();
                    }
                    seen_placeholder = true;
                    trailing_literals.clear();
                }
            }
        }
        // trailing_literals after the last placeholder are part of the suffix, not must_contain

        (min_len, prefix, suffix, must_contain)
    }

    pub fn placeholder_names(&self) -> HashSet<&str> {
        self.tokens
            .iter()
            .filter_map(|t| match t {
                Token::Placeholder(name) => Some(name.as_str()),
                Token::Literal(_) => None,
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn has_placeholders(&self) -> bool {
        self.tokens
            .iter()
            .any(|t| matches!(t, Token::Placeholder(_)))
    }

    /// Returns true if the input definitely cannot match this pattern.
    pub fn fast_reject(&self, input: &str) -> bool {
        if input.len() < self.min_len {
            return true;
        }
        if let Some(ref prefix) = self.prefix {
            if !input.starts_with(prefix.as_str()) {
                return true;
            }
        }
        if let Some(ref suffix) = self.suffix {
            if !input.ends_with(suffix.as_str()) {
                return true;
            }
        }
        for literal in &self.must_contain {
            if !input.contains(literal.as_str()) {
                return true;
            }
        }
        false
    }

    /// Attempt to match the input against this pattern, returning captures.
    ///
    /// Uses greedy-last matching: each placeholder consumes the minimum
    /// non-empty characters needed for remaining literals to match.
    /// The last placeholder (or one before a suffix) consumes the remainder.
    ///
    /// Empty placeholder captures are not allowed.
    pub fn try_match<'a>(&self, input: &'a str) -> Option<HashMap<String, &'a str>> {
        let mut captures = HashMap::new();
        let mut pos = 0;

        for (i, token) in self.tokens.iter().enumerate() {
            match token {
                Token::Literal(lit) => {
                    if !input[pos..].starts_with(lit.as_str()) {
                        return None;
                    }
                    pos += lit.len();
                }
                Token::Placeholder(name) => {
                    // Find the next literal after this placeholder
                    let next_literal = self.next_literal_after(i);

                    let capture_end = match next_literal {
                        Some(next_lit) => {
                            // Find where the next literal starts, searching from pos+1
                            // (placeholder must capture at least 1 char)
                            let search_start = pos + 1;
                            if search_start > input.len() {
                                return None;
                            }
                            let found = input[search_start..].find(next_lit.as_str())?;
                            search_start + found
                        }
                        None => {
                            // Last token or no more literals — consume rest
                            input.len()
                        }
                    };

                    let value = &input[pos..capture_end];
                    if value.is_empty() {
                        return None;
                    }
                    captures.insert(name.clone(), value);
                    pos = capture_end;
                }
            }
        }

        if pos != input.len() {
            return None;
        }

        Some(captures)
    }

    /// Find the concatenated literal string immediately following token index `i`.
    fn next_literal_after(&self, i: usize) -> Option<String> {
        let mut buf = String::new();
        for t in &self.tokens[i + 1..] {
            match t {
                Token::Literal(s) => buf.push_str(s),
                Token::Placeholder(_) => break,
            }
        }
        if buf.is_empty() { None } else { Some(buf) }
    }

    /// Substitute captures into this pattern to produce a concrete string.
    pub fn substitute(&self, captures: &HashMap<String, &str>) -> String {
        let mut out = String::new();
        for token in &self.tokens {
            match token {
                Token::Literal(s) => out.push_str(s),
                Token::Placeholder(name) => out.push_str(captures[name.as_str()]),
            }
        }
        out
    }

    /// The full literal text of this pattern (only valid if no placeholders).
    pub fn literal_text(&self) -> Option<&str> {
        if self.tokens.len() == 1 {
            if let Token::Literal(s) = &self.tokens[0] {
                return Some(s.as_str());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_literal_only() {
        let p = CompiledPattern::parse("/exact/path");
        assert!(!p.has_placeholders());
        assert_eq!(p.literal_text(), Some("/exact/path"));
        assert_eq!(p.min_len, 11);
    }

    #[test]
    fn parse_prefix_pattern() {
        let p = CompiledPattern::parse("/foo/{path}");
        assert_eq!(p.prefix, Some("/foo/".into()));
        assert_eq!(p.suffix, None);
        assert!(p.must_contain.is_empty());
        assert_eq!(p.min_len, 5);
    }

    #[test]
    fn parse_suffix_pattern() {
        let p = CompiledPattern::parse("{bar}-anything");
        assert_eq!(p.prefix, None);
        assert_eq!(p.suffix, Some("-anything".into()));
        assert!(p.must_contain.is_empty());
        assert_eq!(p.min_len, 9);
    }

    #[test]
    fn parse_interior_literal() {
        let p = CompiledPattern::parse("{foo}-bar-{xxx}");
        assert_eq!(p.prefix, None);
        assert_eq!(p.suffix, None);
        assert_eq!(p.must_contain, vec!["-bar-"]);
        assert_eq!(p.min_len, 5);
    }

    #[test]
    fn guard_rejects_too_short() {
        let p = CompiledPattern::parse("/foo/{path}");
        assert!(p.fast_reject("/fo"));
    }

    #[test]
    fn guard_rejects_wrong_prefix() {
        let p = CompiledPattern::parse("/foo/{path}");
        assert!(p.fast_reject("/bar/something"));
    }

    #[test]
    fn guard_rejects_wrong_suffix() {
        let p = CompiledPattern::parse("{bar}-anything");
        assert!(p.fast_reject("hello-something"));
    }

    #[test]
    fn guard_rejects_missing_interior() {
        let p = CompiledPattern::parse("{foo}-bar-{xxx}");
        assert!(p.fast_reject("abc-baz-def"));
    }

    #[test]
    fn guard_accepts_valid() {
        let p = CompiledPattern::parse("{foo}-bar-{xxx}");
        assert!(!p.fast_reject("abc-bar-def"));
    }

    #[test]
    fn try_match_prefix() {
        let p = CompiledPattern::parse("/foo/{path}");
        let caps = p.try_match("/foo/abc").unwrap();
        assert_eq!(caps["path"], "abc");
    }

    #[test]
    fn try_match_suffix() {
        let p = CompiledPattern::parse("{bar}-anything");
        let caps = p.try_match("hello-anything").unwrap();
        assert_eq!(caps["bar"], "hello");
    }

    #[test]
    fn try_match_interior() {
        let p = CompiledPattern::parse("{foo}-bar-{xxx}");
        let caps = p.try_match("abc-bar-def").unwrap();
        assert_eq!(caps["foo"], "abc");
        assert_eq!(caps["xxx"], "def");
    }

    #[test]
    fn try_match_no_match() {
        let p = CompiledPattern::parse("/foo/{path}");
        assert!(p.try_match("/bar/abc").is_none());
    }

    #[test]
    fn try_match_rejects_empty_capture() {
        let p = CompiledPattern::parse("foo-{bar}");
        assert!(p.try_match("foo-").is_none());
    }

    #[test]
    fn try_match_exact() {
        let p = CompiledPattern::parse("/exact/path");
        assert!(p.try_match("/exact/path").is_some());
        assert!(p.try_match("/exact/other").is_none());
    }

    #[test]
    fn substitute_works() {
        let p = CompiledPattern::parse("/real/{foo}/{xxx}");
        let mut caps = HashMap::new();
        caps.insert("foo".to_string(), "abc");
        caps.insert("xxx".to_string(), "def");
        assert_eq!(p.substitute(&caps), "/real/abc/def");
    }

    #[test]
    fn try_match_segment_based() {
        let p = CompiledPattern::parse("sale/{sale_id}/receivables/{user_id}");
        let caps = p.try_match("sale/1/receivables/42").unwrap();
        assert_eq!(caps["sale_id"], "1");
        assert_eq!(caps["user_id"], "42");
    }
}
