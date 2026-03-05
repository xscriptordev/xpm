//! Dependency string parser for ALPM-format dependency specifications.
//!
//! Parses strings like `"glibc>=2.38"`, `"openssl=1.1.1"`, `"bash"` into
//! structured dependency constraints.

use std::fmt;

use crate::resolver::version::Version;

/// Comparison operator in a dependency constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    /// `>=`
    Ge,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `<`
    Lt,
    /// `=`
    Eq,
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operator::Ge => write!(f, ">="),
            Operator::Le => write!(f, "<="),
            Operator::Gt => write!(f, ">"),
            Operator::Lt => write!(f, "<"),
            Operator::Eq => write!(f, "="),
        }
    }
}

/// A parsed dependency constraint, e.g. `glibc>=2.38`.
///
/// If `version` is `None`, the dependency has no version constraint
/// (i.e. any version satisfies it).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DepConstraint {
    /// Package name.
    pub name: String,
    /// Optional version operator.
    pub op: Option<Operator>,
    /// Optional version constraint.
    pub version: Option<Version>,
}

impl DepConstraint {
    /// Parse a dependency string in ALPM format.
    ///
    /// Examples:
    /// - `"glibc"` → name only, any version
    /// - `"glibc>=2.38"` → name with `>=` constraint
    /// - `"openssl=1.1.1-3"` → name with exact version
    pub fn parse(s: &str) -> Self {
        // Try two-char operators first, then single-char
        for (op_str, op) in &[
            (">=", Operator::Ge),
            ("<=", Operator::Le),
            (">", Operator::Gt),
            ("<", Operator::Lt),
            ("=", Operator::Eq),
        ] {
            if let Some(pos) = s.find(op_str) {
                let name = s[..pos].to_string();
                let ver_str = &s[pos + op_str.len()..];
                return Self {
                    name,
                    op: Some(*op),
                    version: Some(Version::parse(ver_str)),
                };
            }
        }

        // No operator found — unconstrained dependency
        Self {
            name: s.to_string(),
            op: None,
            version: None,
        }
    }

    /// Check whether the given version satisfies this constraint.
    ///
    /// Returns `true` if there is no constraint (any version matches).
    pub fn matches(&self, candidate: &Version) -> bool {
        match (&self.op, &self.version) {
            (None, _) | (_, None) => true,
            (Some(op), Some(ver)) => {
                let ord = candidate.cmp(ver);
                match op {
                    Operator::Ge => ord != std::cmp::Ordering::Less,
                    Operator::Le => ord != std::cmp::Ordering::Greater,
                    Operator::Gt => ord == std::cmp::Ordering::Greater,
                    Operator::Lt => ord == std::cmp::Ordering::Less,
                    Operator::Eq => ord == std::cmp::Ordering::Equal,
                }
            }
        }
    }
}

impl fmt::Display for DepConstraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let (Some(op), Some(ver)) = (&self.op, &self.version) {
            write!(f, "{op}{ver}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parsing ──────────────────────────────────────────────────────

    #[test]
    fn parse_name_only() {
        let dep = DepConstraint::parse("bash");
        assert_eq!(dep.name, "bash");
        assert!(dep.op.is_none());
        assert!(dep.version.is_none());
    }

    #[test]
    fn parse_ge() {
        let dep = DepConstraint::parse("glibc>=2.38");
        assert_eq!(dep.name, "glibc");
        assert_eq!(dep.op, Some(Operator::Ge));
        assert_eq!(dep.version.as_ref().unwrap().pkgver, "2.38");
    }

    #[test]
    fn parse_le() {
        let dep = DepConstraint::parse("openssl<=1.1.1");
        assert_eq!(dep.name, "openssl");
        assert_eq!(dep.op, Some(Operator::Le));
    }

    #[test]
    fn parse_gt() {
        let dep = DepConstraint::parse("python>3.10");
        assert_eq!(dep.name, "python");
        assert_eq!(dep.op, Some(Operator::Gt));
    }

    #[test]
    fn parse_lt() {
        let dep = DepConstraint::parse("gcc<13");
        assert_eq!(dep.name, "gcc");
        assert_eq!(dep.op, Some(Operator::Lt));
    }

    #[test]
    fn parse_eq() {
        let dep = DepConstraint::parse("linux=6.2.9-1");
        assert_eq!(dep.name, "linux");
        assert_eq!(dep.op, Some(Operator::Eq));
        assert_eq!(dep.version.as_ref().unwrap().pkgver, "6.2.9");
        assert_eq!(dep.version.as_ref().unwrap().pkgrel, "1");
    }

    #[test]
    fn parse_with_epoch() {
        let dep = DepConstraint::parse("mesa>=1:23.1.0-1");
        assert_eq!(dep.name, "mesa");
        assert_eq!(dep.op, Some(Operator::Ge));
        let ver = dep.version.as_ref().unwrap();
        assert_eq!(ver.epoch, 1);
        assert_eq!(ver.pkgver, "23.1.0");
        assert_eq!(ver.pkgrel, "1");
    }

    #[test]
    fn display_roundtrip() {
        let cases = ["bash", "glibc>=2.38", "linux=6.2.9-1", "mesa>=1:23.1.0-1"];
        for s in cases {
            assert_eq!(DepConstraint::parse(s).to_string(), s);
        }
    }

    // ── Matching ─────────────────────────────────────────────────────

    #[test]
    fn matches_unconstrained() {
        let dep = DepConstraint::parse("bash");
        assert!(dep.matches(&Version::parse("1.0")));
        assert!(dep.matches(&Version::parse("999.0")));
    }

    #[test]
    fn matches_ge() {
        let dep = DepConstraint::parse("glibc>=2.38");
        assert!(dep.matches(&Version::parse("2.38")));
        assert!(dep.matches(&Version::parse("2.39")));
        assert!(!dep.matches(&Version::parse("2.37")));
    }

    #[test]
    fn matches_le() {
        let dep = DepConstraint::parse("pkg<=3.0");
        assert!(dep.matches(&Version::parse("3.0")));
        assert!(dep.matches(&Version::parse("2.9")));
        assert!(!dep.matches(&Version::parse("3.1")));
    }

    #[test]
    fn matches_gt() {
        let dep = DepConstraint::parse("pkg>1.0");
        assert!(dep.matches(&Version::parse("1.1")));
        assert!(!dep.matches(&Version::parse("1.0")));
        assert!(!dep.matches(&Version::parse("0.9")));
    }

    #[test]
    fn matches_lt() {
        let dep = DepConstraint::parse("pkg<2.0");
        assert!(dep.matches(&Version::parse("1.9")));
        assert!(!dep.matches(&Version::parse("2.0")));
        assert!(!dep.matches(&Version::parse("2.1")));
    }

    #[test]
    fn matches_eq() {
        let dep = DepConstraint::parse("linux=6.2.9-1");
        assert!(dep.matches(&Version::parse("6.2.9-1")));
        assert!(!dep.matches(&Version::parse("6.2.9-2")));
        assert!(!dep.matches(&Version::parse("6.2.10-1")));
    }
}
