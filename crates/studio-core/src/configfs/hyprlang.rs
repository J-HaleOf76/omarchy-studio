//! Hyprlang line-oriented CST (spec 03 §2).
//!
//! Hyprland config is line-based: `key = value`, `category { … }` nesting,
//! `$var = value`, `source = path`, `#` comments, and `bind* = …` lines.
//! We model every line, keeping its exact bytes, so `parse(s).to_string()`
//! reproduces `s` verbatim (the round-trip invariant, enforced by golden
//! tests over real Omarchy configs). Edits touch only the changed line;
//! everything else — comments, spacing, unknown directives — is preserved.

/// A parsed hyprlang document. Cheap to build; lossless to serialize.
#[derive(Debug, Clone)]
pub struct HyprDoc {
    lines: Vec<Line>,
    trailing_newline: bool,
}

#[derive(Debug, Clone)]
struct Line {
    raw: String,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Blank,
    Comment,
    CategoryOpen(String),
    CategoryClose,
    /// `$name = value`
    Var,
    /// `source = path`
    Source,
    /// `key = value` inside category `path`. `indent` preserves leading
    /// whitespace for in-place edits.
    Assign {
        path: Vec<String>,
        key: String,
        value: String,
        indent: String,
    },
    /// Anything we don't specifically model — preserved, never touched.
    Other,
}

/// An effective key/value with its category path, e.g.
/// `general.gaps_in = 5` or a `bind = SUPER, T, exec, …` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: Vec<String>,
    pub key: String,
    pub value: String,
}

impl Entry {
    /// Dotted address: `general.gaps_in`. Binds address as just their key
    /// (`bind`, `bindeld`, …) since they aren't unique.
    pub fn dotted(&self) -> String {
        if self.path.is_empty() {
            self.key.clone()
        } else {
            format!("{}.{}", self.path.join("."), self.key)
        }
    }
}

impl HyprDoc {
    pub fn parse(input: &str) -> Self {
        let trailing_newline = input.ends_with('\n');
        // split_inclusive keeps us honest about the last line; strip the '\n'
        // for storage and re-add on serialize.
        let raw_lines: Vec<&str> = if input.is_empty() {
            Vec::new()
        } else {
            input.split('\n').collect()
        };
        // When the file ends with '\n', the final split element is "" — drop it;
        // the trailing newline is tracked separately.
        let raw_lines = if trailing_newline {
            &raw_lines[..raw_lines.len() - 1]
        } else {
            &raw_lines[..]
        };

        let mut stack: Vec<String> = Vec::new();
        let mut lines = Vec::with_capacity(raw_lines.len());
        for raw in raw_lines {
            let kind = classify(raw, &mut stack);
            lines.push(Line {
                raw: raw.to_string(),
                kind,
            });
        }
        Self {
            lines,
            trailing_newline,
        }
    }

    /// Byte-identical serialization of an unedited document.
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        let mut out = self
            .lines
            .iter()
            .map(|l| l.raw.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if self.trailing_newline {
            out.push('\n');
        }
        out
    }

    /// Every assignment (excluding vars/sources/binds), for iteration.
    pub fn entries(&self) -> Vec<Entry> {
        self.lines
            .iter()
            .filter_map(|l| match &l.kind {
                Kind::Assign {
                    path, key, value, ..
                } if !is_bind_key(key) => Some(Entry {
                    path: path.clone(),
                    key: key.clone(),
                    value: value.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// All `bind*` lines in document order (feeds the keybinds module).
    pub fn binds(&self) -> Vec<Entry> {
        self.lines
            .iter()
            .filter_map(|l| match &l.kind {
                Kind::Assign {
                    path, key, value, ..
                } if is_bind_key(key) => Some(Entry {
                    path: path.clone(),
                    key: key.clone(),
                    value: value.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Read a value by dotted address (`general.gaps_in`). First match wins.
    /// Matches against each assignment's full address, so keys that themselves
    /// contain dots (`col.active_border`) resolve unambiguously.
    pub fn get(&self, dotted: &str) -> Option<String> {
        self.lines.iter().find_map(|l| match &l.kind {
            Kind::Assign {
                path, key, value, ..
            } if address(path, key) == dotted => Some(value.clone()),
            _ => None,
        })
    }

    /// Set a value by dotted address, in place, preserving indentation.
    /// Returns false if the key isn't present (caller decides whether to append).
    pub fn set(&mut self, dotted: &str, new_value: &str) -> bool {
        for l in &mut self.lines {
            if let Kind::Assign {
                path,
                key,
                value,
                indent,
            } = &mut l.kind
            {
                if address(path, key) == dotted {
                    *value = new_value.to_string();
                    l.raw = format!("{indent}{key} = {new_value}");
                    return true;
                }
            }
        }
        false
    }
}

fn classify(raw: &str, stack: &mut Vec<String>) -> Kind {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Kind::Blank;
    }
    if trimmed.starts_with('#') {
        return Kind::Comment;
    }
    if trimmed == "}" {
        stack.pop();
        return Kind::CategoryClose;
    }
    // `name {` — a category open (name may be empty for anonymous blocks).
    if let Some(name) = trimmed.strip_suffix('{') {
        let name = name.trim().to_string();
        stack.push(name.clone());
        return Kind::CategoryOpen(name);
    }
    if let Some((lhs, rhs)) = trimmed.split_once('=') {
        let key = lhs.trim().to_string();
        let value = rhs.trim().to_string();
        if key.starts_with('$') {
            return Kind::Var;
        }
        if key == "source" {
            return Kind::Source;
        }
        let indent: String = raw.chars().take_while(|c| c.is_whitespace()).collect();
        return Kind::Assign {
            path: stack.clone(),
            key,
            value,
            indent,
        };
    }
    Kind::Other
}

fn is_bind_key(key: &str) -> bool {
    key.starts_with("bind") || key.starts_with("unbind")
}

/// Full dotted address of an assignment: `general` + `col.active_border`
/// -> `general.col.active_border`; empty path -> just the key.
fn address(path: &[String], key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", path.join("."), key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NESTED: &str = "\
# comment
$var = rgba(112233ff)

general {
    gaps_in = 5
    col.active_border = $var

    decoration {
        rounding = 0
    }
}

bind = SUPER, T, exec, foot
";

    #[test]
    fn round_trips_byte_identical() {
        for s in ["", "\n", "a = b", "a = b\n", NESTED] {
            assert_eq!(
                HyprDoc::parse(s).to_string(),
                s,
                "round-trip failed for {s:?}"
            );
        }
    }

    #[test]
    fn reads_nested_and_dotted_keys() {
        let d = HyprDoc::parse(NESTED);
        assert_eq!(d.get("general.gaps_in").as_deref(), Some("5"));
        assert_eq!(d.get("general.decoration.rounding").as_deref(), Some("0"));
        assert_eq!(d.get("general.col.active_border").as_deref(), Some("$var"));
        assert_eq!(d.get("nonexistent.key"), None);
    }

    #[test]
    fn set_edits_in_place_preserving_indent_and_rest() {
        let mut d = HyprDoc::parse(NESTED);
        assert!(d.set("general.decoration.rounding", "8"));
        let out = d.to_string();
        assert!(
            out.contains("        rounding = 8"),
            "indent preserved: {out}"
        );
        // everything else identical
        assert!(out.contains("# comment"));
        assert!(out.contains("bind = SUPER, T, exec, foot"));
        // setting an absent key reports false, changes nothing
        let before = d.to_string();
        assert!(!d.set("general.nope", "x"));
        assert_eq!(d.to_string(), before);
    }

    #[test]
    fn separates_binds_from_assignments() {
        let d = HyprDoc::parse(NESTED);
        let binds = d.binds();
        assert_eq!(binds.len(), 1);
        assert_eq!(binds[0].key, "bind");
        assert_eq!(binds[0].value, "SUPER, T, exec, foot");
        // binds are excluded from entries()
        assert!(d.entries().iter().all(|e| e.key != "bind"));
        assert!(d.entries().iter().any(|e| e.dotted() == "general.gaps_in"));
    }
}
