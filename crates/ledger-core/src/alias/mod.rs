//! Fast alias/rewrite matcher with indexed lookups.
//!
//! [`AliasMatcher`] registers rewrite rules with `{placeholder}` patterns and
//! matches incoming strings against those rules. Indexes (prefix, suffix,
//! contains, exact) quickly discard rules that cannot match, so only a small
//! candidate set reaches full validation.
//!
//! Registration order defines priority — the first registered matching rule wins.
//!
//! [`AliasRegistry`] is a type alias for backward compatibility.
//!
//! ```
//! # use ledger_core::AliasRegistry;
//! let mut reg = AliasRegistry::new();
//! reg.register(
//!     "user/{user_id}/to_pay/{sale_id}",     // canonical
//!     "sale/{sale_id}/receivables/{user_id}", // alias
//! ).unwrap();
//!
//! assert_eq!(reg.resolve("sale/1/receivables/42"), "user/42/to_pay/1");
//! assert_eq!(reg.resolve("warehouse/1"), "warehouse/1"); // no match → unchanged
//! ```

mod index;
mod pattern;

use std::collections::HashMap;
use std::fmt;

use index::RuleIndex;
use pattern::CompiledPattern;

/// Error returned when registering an alias with mismatched placeholders.
#[derive(Debug, Clone)]
pub struct AliasError {
    pub canonical: String,
    pub alias: String,
    pub message: String,
}

impl fmt::Display for AliasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "alias error between '{}' and '{}': {}",
            self.canonical, self.alias, self.message
        )
    }
}

impl std::error::Error for AliasError {}

/// Result of a successful lookup.
#[derive(Debug, Clone)]
pub struct Match {
    pub rule_id: usize,
    pub captures: HashMap<String, String>,
    pub rewritten: String,
}

#[derive(Debug)]
struct Rule {
    source: CompiledPattern,
    target: CompiledPattern,
}

/// Fast alias/rewrite matcher with indexed lookups.
///
/// Rules are registered in order. The first registered rule that fully matches
/// the input wins. Indexes are used to quickly discard non-matching rules.
#[derive(Debug, Default)]
pub struct AliasMatcher {
    rules: Vec<Rule>,
    index: RuleIndex,
}

/// Backward-compatible alias for [`AliasMatcher`].
pub type AliasRegistry = AliasMatcher;

impl AliasMatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a canonical ↔ alias rule.
    ///
    /// The `alias` pattern is matched against inputs. When it matches,
    /// captured placeholders are substituted into the `canonical` pattern
    /// to produce the rewritten output.
    ///
    /// Both templates must contain the same set of `{name}` placeholders.
    pub fn register(&mut self, canonical: &str, alias: &str) -> Result<(), AliasError> {
        let source = CompiledPattern::parse(alias);
        let target = CompiledPattern::parse(canonical);

        let sn = source.placeholder_names();
        let tn = target.placeholder_names();

        if sn != tn {
            let only_canonical: Vec<_> = tn.difference(&sn).copied().collect();
            let only_alias: Vec<_> = sn.difference(&tn).copied().collect();
            let mut msg = String::from("placeholder mismatch");
            if !only_canonical.is_empty() {
                msg.push_str(&format!(
                    " — only in canonical: {{{}}}",
                    only_canonical.join("}, {")
                ));
            }
            if !only_alias.is_empty() {
                msg.push_str(&format!(
                    " — only in alias: {{{}}}",
                    only_alias.join("}, {")
                ));
            }
            return Err(AliasError {
                canonical: canonical.to_string(),
                alias: alias.to_string(),
                message: msg,
            });
        }

        let rule_id = self.rules.len();
        self.index.insert(rule_id, &source);
        self.rules.push(Rule { source, target });
        Ok(())
    }

    /// Look up an input and return the full match result with captures.
    pub fn lookup(&self, input: &str) -> Option<Match> {
        let candidates = self.index.candidates(input);

        for rule_id in candidates {
            let rule = &self.rules[rule_id];

            if rule.source.fast_reject(input) {
                continue;
            }

            if let Some(captures) = rule.source.try_match(input) {
                let rewritten = rule.target.substitute(&captures);
                return Some(Match {
                    rule_id,
                    captures: captures
                        .into_iter()
                        .map(|(k, v)| (k, v.to_string()))
                        .collect(),
                    rewritten,
                });
            }
        }

        None
    }

    /// Resolve an input to its rewritten form, or return unchanged.
    pub fn resolve(&self, input: &str) -> String {
        self.lookup(input)
            .map(|m| m.rewritten)
            .unwrap_or_else(|| input.to_string())
    }
}

impl Default for RuleIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Ported from the original alias.rs ───────────────────────────────

    fn registry() -> AliasRegistry {
        let mut reg = AliasRegistry::new();
        reg.register(
            "user/{user_id}/to_pay/{sale_id}",
            "sale/{sale_id}/receivables/{user_id}",
        )
        .unwrap();
        reg
    }

    #[test]
    fn resolve_alias_to_canonical() {
        let reg = registry();
        assert_eq!(reg.resolve("sale/1/receivables/42"), "user/42/to_pay/1");
    }

    #[test]
    fn resolve_canonical_unchanged() {
        let reg = registry();
        assert_eq!(reg.resolve("user/42/to_pay/1"), "user/42/to_pay/1");
    }

    #[test]
    fn resolve_no_match_unchanged() {
        let reg = registry();
        assert_eq!(reg.resolve("warehouse/1"), "warehouse/1");
    }

    #[test]
    fn resolve_wrong_segment_count() {
        let reg = registry();
        assert_eq!(reg.resolve("sale/1/receivables"), "sale/1/receivables");
    }

    #[test]
    fn register_rejects_missing_placeholder_in_alias() {
        let mut reg = AliasRegistry::new();
        let err = reg
            .register("a/{x}/b/{y}", "c/{x}/d")
            .expect_err("should fail");
        assert!(err.message.contains("y"), "should mention missing {{y}}");
    }

    #[test]
    fn register_rejects_missing_placeholder_in_canonical() {
        let mut reg = AliasRegistry::new();
        let err = reg
            .register("a/{x}", "b/{x}/c/{y}")
            .expect_err("should fail");
        assert!(err.message.contains("y"), "should mention missing {{y}}");
    }

    #[test]
    fn register_accepts_matching_placeholders() {
        let mut reg = AliasRegistry::new();
        reg.register("a/{x}/b/{y}", "c/{y}/d/{x}").unwrap();
    }

    #[test]
    fn multiple_templates() {
        let mut reg = AliasRegistry::new();
        reg.register(
            "user/{user_id}/to_pay/{sale_id}",
            "sale/{sale_id}/receivables/{user_id}",
        )
        .unwrap();
        reg.register(
            "warehouse/{wh}/product/{pid}",
            "product/{pid}/warehouse/{wh}",
        )
        .unwrap();

        assert_eq!(
            reg.resolve("product/10/warehouse/3"),
            "warehouse/3/product/10"
        );
        assert_eq!(
            reg.resolve("sale/5/receivables/99"),
            "user/99/to_pay/5"
        );
    }

    // ── New tests for the indexed matcher ───────────────────────────────

    #[test]
    fn inline_placeholder_prefix() {
        let mut m = AliasMatcher::new();
        m.register("bar/{bar}", "foo-{bar}").unwrap();
        let r = m.lookup("foo-hello").unwrap();
        assert_eq!(r.captures["bar"], "hello");
        assert_eq!(r.rewritten, "bar/hello");
    }

    #[test]
    fn inline_placeholder_suffix() {
        let mut m = AliasMatcher::new();
        m.register("anything/{bar}", "{bar}-anything").unwrap();
        let r = m.lookup("hello-anything").unwrap();
        assert_eq!(r.captures["bar"], "hello");
        assert_eq!(r.rewritten, "anything/hello");
    }

    #[test]
    fn inline_placeholder_interior() {
        let mut m = AliasMatcher::new();
        m.register("{foo}/{xxx}", "{foo}-bar-{xxx}").unwrap();
        let r = m.lookup("abc-bar-def").unwrap();
        assert_eq!(r.captures["foo"], "abc");
        assert_eq!(r.captures["xxx"], "def");
        assert_eq!(r.rewritten, "abc/def");
    }

    #[test]
    fn registration_order_first_wins() {
        let mut m = AliasMatcher::new();
        m.register("target0/{foo}/{xxx}", "{foo}-bar-{xxx}").unwrap();
        m.register("target1/{bar}", "foo-{bar}").unwrap();

        let r = m.lookup("foo-bar-baz").unwrap();
        assert_eq!(r.rule_id, 0);
        assert_eq!(r.rewritten, "target0/foo/baz");
    }

    #[test]
    fn registration_order_reversed() {
        let mut m = AliasMatcher::new();
        m.register("target0/{bar}", "foo-{bar}").unwrap();
        m.register("target1/{foo}/{xxx}", "{foo}-bar-{xxx}").unwrap();

        let r = m.lookup("foo-bar-baz").unwrap();
        assert_eq!(r.rule_id, 0);
        assert_eq!(r.rewritten, "target0/bar-baz");
    }

    #[test]
    fn exact_rule() {
        let mut m = AliasMatcher::new();
        m.register("/real/path", "/exact/match").unwrap();
        let r = m.lookup("/exact/match").unwrap();
        assert_eq!(r.rewritten, "/real/path");
    }

    #[test]
    fn no_match_returns_none() {
        let mut m = AliasMatcher::new();
        m.register("target/{x}", "foo-{x}").unwrap();
        assert!(m.lookup("bar-hello").is_none());
    }

    #[test]
    fn empty_capture_rejected() {
        let mut m = AliasMatcher::new();
        m.register("target/{x}", "foo-{x}").unwrap();
        assert!(m.lookup("foo-").is_none());
    }

    #[test]
    fn slow_rule_catch_all() {
        let mut m = AliasMatcher::new();
        m.register("caught/{x}", "{x}").unwrap();
        let r = m.lookup("anything").unwrap();
        assert_eq!(r.rewritten, "caught/anything");
    }

    #[test]
    fn slow_rule_priority_over_later_optimized() {
        let mut m = AliasMatcher::new();
        m.register("caught/{x}", "{x}").unwrap();
        m.register("target/{path}", "/foo/{path}").unwrap();

        let r = m.lookup("/foo/test").unwrap();
        assert_eq!(r.rule_id, 0, "catch-all registered first must win");
        assert_eq!(r.rewritten, "caught//foo/test");
    }

    #[test]
    fn multiple_rules_same_prefix() {
        let mut m = AliasMatcher::new();
        m.register("t1/{x}", "/api/v1/{x}").unwrap();
        m.register("t2/{x}", "/api/v2/{x}").unwrap();

        assert_eq!(m.resolve("/api/v1/users"), "t1/users");
        assert_eq!(m.resolve("/api/v2/users"), "t2/users");
    }

    #[test]
    fn multiple_rules_same_suffix() {
        let mut m = AliasMatcher::new();
        m.register("t1/{x}", "{x}.json").unwrap();
        m.register("t2/{x}", "{x}.xml").unwrap();

        assert_eq!(m.resolve("data.json"), "t1/data");
        assert_eq!(m.resolve("data.xml"), "t2/data");
    }

    #[test]
    fn lookup_returns_captures() {
        let mut m = AliasMatcher::new();
        m.register("out/{a}/{b}", "{a}-mid-{b}").unwrap();
        let r = m.lookup("hello-mid-world").unwrap();
        assert_eq!(r.captures.len(), 2);
        assert_eq!(r.captures["a"], "hello");
        assert_eq!(r.captures["b"], "world");
    }
}
