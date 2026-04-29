use std::collections::HashMap;

use super::pattern::CompiledPattern;

/// Indexes for fast candidate selection.
///
/// Each rule is stored in exactly one index based on its pattern shape.
/// At lookup time, candidates are collected from all relevant indexes,
/// merged with fallback rules, and sorted by registration order.
#[derive(Debug)]
pub(crate) struct RuleIndex {
    exact: HashMap<String, Vec<usize>>,
    prefix: HashMap<String, Vec<usize>>,
    suffix: HashMap<String, Vec<usize>>,
    contains: HashMap<String, Vec<usize>>,
    prefix_lengths: Vec<usize>,
    suffix_lengths: Vec<usize>,
    fallback: Vec<usize>,
}

impl RuleIndex {
    pub fn new() -> Self {
        Self {
            exact: HashMap::new(),
            prefix: HashMap::new(),
            suffix: HashMap::new(),
            contains: HashMap::new(),
            prefix_lengths: Vec::new(),
            suffix_lengths: Vec::new(),
            fallback: Vec::new(),
        }
    }

    /// Insert a rule into the best available index.
    pub fn insert(&mut self, rule_id: usize, pattern: &CompiledPattern) {
        // No placeholders → exact match
        if let Some(text) = pattern.literal_text() {
            self.exact.entry(text.to_string()).or_default().push(rule_id);
            return;
        }

        // Has a fixed prefix
        if let Some(ref prefix) = pattern.prefix {
            let len = prefix.len();
            self.prefix
                .entry(prefix.clone())
                .or_default()
                .push(rule_id);
            if !self.prefix_lengths.contains(&len) {
                self.prefix_lengths.push(len);
                self.prefix_lengths.sort_unstable();
            }
            return;
        }

        // Has a fixed suffix
        if let Some(ref suffix) = pattern.suffix {
            let len = suffix.len();
            self.suffix
                .entry(suffix.clone())
                .or_default()
                .push(rule_id);
            if !self.suffix_lengths.contains(&len) {
                self.suffix_lengths.push(len);
                self.suffix_lengths.sort_unstable();
            }
            return;
        }

        // Has an interior literal anchor
        if let Some(anchor) = pattern.must_contain.first() {
            self.contains
                .entry(anchor.clone())
                .or_default()
                .push(rule_id);
            return;
        }

        // No useful literal — fallback (e.g. `{anything}`)
        self.fallback.push(rule_id);
    }

    /// Collect candidate rule IDs for a given input.
    ///
    /// Returns candidates sorted by rule_id (registration order) and deduped.
    pub fn candidates(&self, input: &str) -> Vec<usize> {
        let mut out = Vec::new();

        // Exact
        if let Some(ids) = self.exact.get(input) {
            out.extend(ids);
        }

        // Prefix: only check known prefix lengths
        for &len in &self.prefix_lengths {
            if len > input.len() {
                break;
            }
            if let Some(prefix) = input.get(..len) {
                if let Some(ids) = self.prefix.get(prefix) {
                    out.extend(ids);
                }
            }
        }

        // Suffix: only check known suffix lengths
        for &len in &self.suffix_lengths {
            if len > input.len() {
                break;
            }
            let start = input.len() - len;
            if let Some(suffix) = input.get(start..) {
                if let Some(ids) = self.suffix.get(suffix) {
                    out.extend(ids);
                }
            }
        }

        // Contains
        for (needle, ids) in &self.contains {
            if input.contains(needle.as_str()) {
                out.extend(ids);
            }
        }

        // Fallback — always included
        out.extend(&self.fallback);

        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_index() {
        let mut idx = RuleIndex::new();
        let p = CompiledPattern::parse("/exact");
        idx.insert(0, &p);
        assert_eq!(idx.candidates("/exact"), vec![0]);
        assert!(idx.candidates("/other").is_empty());
    }

    #[test]
    fn prefix_index() {
        let mut idx = RuleIndex::new();
        let p = CompiledPattern::parse("/foo/{x}");
        idx.insert(0, &p);
        assert_eq!(idx.candidates("/foo/bar"), vec![0]);
        assert!(idx.candidates("/bar/x").is_empty());
    }

    #[test]
    fn suffix_index() {
        let mut idx = RuleIndex::new();
        let p = CompiledPattern::parse("{x}-end");
        idx.insert(0, &p);
        assert_eq!(idx.candidates("hello-end"), vec![0]);
        assert!(idx.candidates("hello-start").is_empty());
    }

    #[test]
    fn contains_index() {
        let mut idx = RuleIndex::new();
        let p = CompiledPattern::parse("{a}-mid-{b}");
        idx.insert(0, &p);
        assert_eq!(idx.candidates("x-mid-y"), vec![0]);
        assert!(idx.candidates("x-other-y").is_empty());
    }

    #[test]
    fn fallback_always_included() {
        let mut idx = RuleIndex::new();
        let p = CompiledPattern::parse("{anything}");
        idx.insert(0, &p);
        assert_eq!(idx.candidates("literally anything"), vec![0]);
    }

    #[test]
    fn candidates_sorted_by_rule_id() {
        let mut idx = RuleIndex::new();
        idx.insert(0, &CompiledPattern::parse("{anything}"));
        idx.insert(1, &CompiledPattern::parse("/foo/{x}"));
        idx.insert(2, &CompiledPattern::parse("{a}-foo-{b}"));

        let c = idx.candidates("/foo/bar");
        // rule 0 (fallback) and rule 1 (prefix) should both appear, sorted
        assert!(c.contains(&0));
        assert!(c.contains(&1));
    }
}
