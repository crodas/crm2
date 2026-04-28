//! Template-based account aliases.
//!
//! An [`AliasRegistry`] holds a list of template pairs. Each pair maps an
//! alias form to a canonical form. [`resolve`](AliasRegistry::resolve)
//! translates concrete account paths from alias form to canonical form.
//!
//! Templates use `{name}` placeholders that match a single path segment.
//! Both sides must declare the same set of placeholders.
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

use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Debug, Clone)]
struct AliasTemplate {
    canonical_parts: Vec<Part>,
    alias_parts: Vec<Part>,
}

#[derive(Debug, Clone)]
enum Part {
    Literal(String),
    Capture(String),
}

fn parse_template(template: &str) -> Vec<Part> {
    template
        .split('/')
        .map(|seg| {
            if seg.starts_with('{') && seg.ends_with('}') {
                Part::Capture(seg[1..seg.len() - 1].to_string())
            } else {
                Part::Literal(seg.to_string())
            }
        })
        .collect()
}

fn placeholder_names(parts: &[Part]) -> HashSet<&str> {
    parts
        .iter()
        .filter_map(|p| match p {
            Part::Capture(name) => Some(name.as_str()),
            Part::Literal(_) => None,
        })
        .collect()
}

fn try_match<'a>(parts: &[Part], segments: &[&'a str]) -> Option<HashMap<String, &'a str>> {
    if parts.len() != segments.len() {
        return None;
    }
    let mut captures = HashMap::new();
    for (part, seg) in parts.iter().zip(segments) {
        match part {
            Part::Literal(lit) => {
                if lit != seg {
                    return None;
                }
            }
            Part::Capture(name) => {
                captures.insert(name.clone(), *seg);
            }
        }
    }
    Some(captures)
}

fn substitute(parts: &[Part], captures: &HashMap<String, &str>) -> String {
    parts
        .iter()
        .map(|p| match p {
            Part::Literal(lit) => lit.as_str(),
            Part::Capture(name) => captures[name.as_str()],
        })
        .collect::<Vec<_>>()
        .join("/")
}

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

/// A list of alias rules. Resolves alias account paths to canonical form.
#[derive(Debug, Clone, Default)]
pub struct AliasRegistry {
    templates: Vec<AliasTemplate>,
}

impl AliasRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a canonical ↔ alias rule.
    ///
    /// Both templates must contain the same set of `{name}` placeholders.
    pub fn register(&mut self, canonical: &str, alias: &str) -> Result<(), AliasError> {
        let canonical_parts = parse_template(canonical);
        let alias_parts = parse_template(alias);

        let cn = placeholder_names(&canonical_parts);
        let an = placeholder_names(&alias_parts);

        if cn != an {
            let only_canonical: Vec<_> = cn.difference(&an).copied().collect();
            let only_alias: Vec<_> = an.difference(&cn).copied().collect();
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

        self.templates.push(AliasTemplate {
            canonical_parts,
            alias_parts,
        });
        Ok(())
    }

    /// Resolve an account to its canonical form.
    ///
    /// - Matches alias template → returns canonical form
    /// - No match → returns input unchanged
    pub fn resolve(&self, account: &str) -> String {
        let segments: Vec<&str> = account.split('/').collect();
        for tmpl in &self.templates {
            if let Some(captures) = try_match(&tmpl.alias_parts, &segments) {
                return substitute(&tmpl.canonical_parts, &captures);
            }
        }
        account.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
