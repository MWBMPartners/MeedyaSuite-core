// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Filename template engine — shared across MeedyaConverter / MeedyaDL /
// MeedyaManager for composing filenames from tag values.
//
// Templates use `{name}` for variable substitution, `|` to pipe through
// transformations, and `:NN` for width specifiers. Example:
//
//   "{tracknumber:02} - {artist|fallback:albumartist} - {title|sanitize}.{ext}"
//
// → "03 - Aphex Twin - Selected Ambient Works.flac"
//
// The engine is format-agnostic — `TagSource` is a trait the caller
// implements to wire variable lookup to any tag system (lofty / mp4ameta /
// plain HashMap / etc.).

use std::collections::HashMap;

// ============================================================
// Public Types
// ============================================================

/// A parsed template, ready to render against any [`TagSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    nodes: Vec<Node>,
}

/// Errors that can occur during template parsing or rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// Unclosed `{`.
    UnclosedPlaceholder { column: usize },
    /// `}` without matching `{`.
    UnexpectedCloseBrace { column: usize },
    /// Empty placeholder `{}`.
    EmptyPlaceholder { column: usize },
    /// Unknown transformation name.
    UnknownTransform { column: usize, name: String },
    /// Width specifier wasn't a valid number.
    InvalidWidthSpec { column: usize, raw: String },
    /// Variable lookup returned None and the placeholder had no fallback.
    /// Surfaced from `render` rather than `parse`.
    MissingVariable { name: String },
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnclosedPlaceholder { column } => {
                write!(f, "unclosed `{{` at column {column}")
            }
            Self::UnexpectedCloseBrace { column } => {
                write!(f, "unexpected `}}` at column {column}")
            }
            Self::EmptyPlaceholder { column } => {
                write!(f, "empty `{{}}` at column {column}")
            }
            Self::UnknownTransform { column, name } => {
                write!(f, "unknown transformation `{name}` at column {column}")
            }
            Self::InvalidWidthSpec { column, raw } => {
                write!(f, "invalid width specifier `:{raw}` at column {column}")
            }
            Self::MissingVariable { name } => {
                write!(f, "variable `{name}` not found and has no fallback")
            }
        }
    }
}

impl std::error::Error for TemplateError {}

/// Source of variable values for template rendering.
///
/// Implementations: `HashMap<String, String>`, `HashMap<&str, &str>`,
/// and any caller-provided type that knows how to look up a tag string
/// by name. Lofty / mp4ameta integrations would wrap their `Tag` types
/// in a thin newtype implementing this.
pub trait TagSource {
    fn get(&self, name: &str) -> Option<String>;
}

impl TagSource for HashMap<String, String> {
    fn get(&self, name: &str) -> Option<String> {
        HashMap::get(self, name).cloned()
    }
}

impl TagSource for HashMap<&'static str, &'static str> {
    fn get(&self, name: &str) -> Option<String> {
        HashMap::get(self, name).map(|s| (*s).to_owned())
    }
}

// ============================================================
// AST
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Literal(String),
    Placeholder(Placeholder),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Placeholder {
    name: String,
    /// Zero-pad width for numerics or truncate width for strings.
    width: Option<usize>,
    /// Pipe of transformations applied in order.
    pipe: Vec<Transform>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Transform {
    Sanitize,
    Ascii,
    Lower,
    Upper,
    Title,
    Trim,
    Round,
    Fallback(String),
    Max(usize),
}

// ============================================================
// Parser
// ============================================================

impl Template {
    /// Parse a template string.
    pub fn parse(template: &str) -> Result<Self, TemplateError> {
        let mut nodes = Vec::new();
        let mut buf = String::new();
        let chars: Vec<char> = template.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];
            match ch {
                '{' => {
                    if !buf.is_empty() {
                        nodes.push(Node::Literal(std::mem::take(&mut buf)));
                    }
                    let placeholder_start = i + 1;
                    let close = find_matching_close(&chars, i + 1)
                        .ok_or(TemplateError::UnclosedPlaceholder { column: i + 1 })?;
                    let inner: String = chars[placeholder_start..close].iter().collect();
                    if inner.trim().is_empty() {
                        return Err(TemplateError::EmptyPlaceholder { column: i + 1 });
                    }
                    nodes.push(Node::Placeholder(parse_placeholder(
                        &inner,
                        placeholder_start + 1,
                    )?));
                    i = close + 1;
                }
                '}' => {
                    return Err(TemplateError::UnexpectedCloseBrace { column: i + 1 });
                }
                _ => {
                    buf.push(ch);
                    i += 1;
                }
            }
        }

        if !buf.is_empty() {
            nodes.push(Node::Literal(buf));
        }
        Ok(Self { nodes })
    }

    /// Render the template against `source`. Returns an error if any
    /// placeholder's variable is missing and has no `fallback` transform.
    pub fn render<S: TagSource>(&self, source: &S) -> Result<String, TemplateError> {
        let mut out = String::new();
        for node in &self.nodes {
            match node {
                Node::Literal(s) => out.push_str(s),
                Node::Placeholder(p) => {
                    let value = resolve_placeholder(p, source)?;
                    out.push_str(&value);
                }
            }
        }
        Ok(out)
    }
}

fn find_matching_close(chars: &[char], start: usize) -> Option<usize> {
    // No nested placeholders supported in this version — simple scan for `}`.
    for (offset, ch) in chars[start..].iter().enumerate() {
        if *ch == '}' {
            return Some(start + offset);
        }
    }
    None
}

fn parse_placeholder(inner: &str, base_column: usize) -> Result<Placeholder, TemplateError> {
    let mut pipes = inner.split('|');
    let first = pipes.next().unwrap_or("").trim();

    // First pipe segment is "name" or "name:width".
    let (name, width) = if let Some((name, width_str)) = first.split_once(':') {
        let name = name.trim();
        let width_str = width_str.trim();
        let w: usize = width_str
            .parse()
            .map_err(|_| TemplateError::InvalidWidthSpec {
                column: base_column,
                raw: width_str.to_owned(),
            })?;
        (name.to_owned(), Some(w))
    } else {
        (first.to_owned(), None)
    };

    if name.is_empty() {
        return Err(TemplateError::EmptyPlaceholder {
            column: base_column,
        });
    }

    let mut pipe = Vec::new();
    for raw in pipes {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        pipe.push(parse_transform(raw, base_column)?);
    }

    Ok(Placeholder { name, width, pipe })
}

fn parse_transform(raw: &str, base_column: usize) -> Result<Transform, TemplateError> {
    if let Some(rest) = raw.strip_prefix("fallback:") {
        let other = rest.trim();
        if other.is_empty() {
            return Err(TemplateError::UnknownTransform {
                column: base_column,
                name: "fallback:<empty>".to_owned(),
            });
        }
        return Ok(Transform::Fallback(other.to_owned()));
    }
    if let Some(rest) = raw.strip_prefix("max:") {
        let n: usize = rest
            .trim()
            .parse()
            .map_err(|_| TemplateError::UnknownTransform {
                column: base_column,
                name: format!("max:{}", rest.trim()),
            })?;
        return Ok(Transform::Max(n));
    }
    Ok(match raw {
        "sanitize" => Transform::Sanitize,
        "ascii" => Transform::Ascii,
        "lower" => Transform::Lower,
        "upper" => Transform::Upper,
        "title" => Transform::Title,
        "trim" => Transform::Trim,
        "round" => Transform::Round,
        other => {
            return Err(TemplateError::UnknownTransform {
                column: base_column,
                name: other.to_owned(),
            })
        }
    })
}

// ============================================================
// Rendering
// ============================================================

fn resolve_placeholder<S: TagSource>(p: &Placeholder, source: &S) -> Result<String, TemplateError> {
    let raw = source.get(&p.name);

    // Apply fallback transform proactively if raw is None.
    let mut value = match raw {
        Some(v) => v,
        None => {
            // Look for fallback in the pipe.
            let fallback = p.pipe.iter().find_map(|t| {
                if let Transform::Fallback(other) = t {
                    Some(other.clone())
                } else {
                    None
                }
            });
            match fallback {
                Some(other) => {
                    source
                        .get(&other)
                        .ok_or_else(|| TemplateError::MissingVariable {
                            name: format!("{} (fallback to {})", p.name, other),
                        })?
                }
                None => {
                    return Err(TemplateError::MissingVariable {
                        name: p.name.clone(),
                    })
                }
            }
        }
    };

    // Run other transforms in declared order.
    for t in &p.pipe {
        value = apply_transform(t, &value);
    }

    // Apply width: zero-pad numerics, truncate strings.
    if let Some(width) = p.width {
        if value.chars().all(|c| c.is_ascii_digit()) {
            while value.len() < width {
                value.insert(0, '0');
            }
        } else if value.chars().count() > width {
            value = value.chars().take(width).collect();
        }
    }

    Ok(value)
}

fn apply_transform(t: &Transform, v: &str) -> String {
    match t {
        Transform::Sanitize => sanitize(v),
        Transform::Ascii => ascii_fold(v),
        Transform::Lower => v.to_lowercase(),
        Transform::Upper => v.to_uppercase(),
        Transform::Title => title_case(v),
        Transform::Trim => v.trim().to_owned(),
        Transform::Round => round_numeric(v),
        Transform::Fallback(_) => v.to_owned(), // already handled in resolve
        Transform::Max(n) => v.chars().take(*n).collect(),
    }
}

fn sanitize(v: &str) -> String {
    v.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

fn ascii_fold(v: &str) -> String {
    // Simple NFD-style fold: strip combining marks. This is not a full
    // Unicode-normalization implementation (we don't pull in unicode-normalization),
    // but it handles the common Latin-script case of "é" → "e", "ñ" → "n", etc.
    // Falls through unchanged for non-decomposable characters.
    let mut out = String::with_capacity(v.len());
    for ch in v.chars() {
        match ch {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => out.push('a'),
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' => out.push('A'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' => out.push('e'),
            'È' | 'É' | 'Ê' | 'Ë' | 'Ē' => out.push('E'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' => out.push('i'),
            'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' => out.push('I'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => out.push('o'),
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' => out.push('O'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => out.push('u'),
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' => out.push('U'),
            'ñ' => out.push('n'),
            'Ñ' => out.push('N'),
            'ç' => out.push('c'),
            'Ç' => out.push('C'),
            'ý' | 'ÿ' => out.push('y'),
            'Ý' => out.push('Y'),
            ch => out.push(ch),
        }
    }
    out
}

fn title_case(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut upper_next = true;
    for ch in v.chars() {
        if ch.is_whitespace() || ch == '-' || ch == '_' {
            out.push(ch);
            upper_next = true;
        } else if upper_next {
            for u in ch.to_uppercase() {
                out.push(u);
            }
            upper_next = false;
        } else {
            for l in ch.to_lowercase() {
                out.push(l);
            }
        }
    }
    out
}

fn round_numeric(v: &str) -> String {
    if let Ok(f) = v.parse::<f64>() {
        (f.round() as i64).to_string()
    } else {
        v.to_owned()
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn src(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    // ---- Parser ----

    #[test]
    fn parse_simple() {
        let t = Template::parse("hello {name}").unwrap();
        let rendered = t.render(&src(&[("name", "world")])).unwrap();
        assert_eq!(rendered, "hello world");
    }

    #[test]
    fn parse_multiple_placeholders() {
        let t = Template::parse("{artist} - {title}").unwrap();
        let s = src(&[("artist", "Aphex"), ("title", "Xtal")]);
        assert_eq!(t.render(&s).unwrap(), "Aphex - Xtal");
    }

    #[test]
    fn parse_empty_placeholder_errors() {
        assert!(matches!(
            Template::parse("hello {}"),
            Err(TemplateError::EmptyPlaceholder { column: 7 })
        ));
    }

    #[test]
    fn parse_unclosed_brace_errors() {
        assert!(matches!(
            Template::parse("hello {name"),
            Err(TemplateError::UnclosedPlaceholder { .. })
        ));
    }

    #[test]
    fn parse_unexpected_close_errors() {
        assert!(matches!(
            Template::parse("hello }"),
            Err(TemplateError::UnexpectedCloseBrace { .. })
        ));
    }

    #[test]
    fn parse_unknown_transform_errors() {
        assert!(matches!(
            Template::parse("{name|whatever}"),
            Err(TemplateError::UnknownTransform { .. })
        ));
    }

    #[test]
    fn parse_invalid_width_errors() {
        assert!(matches!(
            Template::parse("{name:abc}"),
            Err(TemplateError::InvalidWidthSpec { .. })
        ));
    }

    // ---- Rendering — basic ----

    #[test]
    fn missing_var_without_fallback_errors() {
        let t = Template::parse("{title}").unwrap();
        let err = t.render(&src(&[])).unwrap_err();
        assert!(matches!(err, TemplateError::MissingVariable { .. }));
    }

    #[test]
    fn fallback_uses_other_var() {
        let t = Template::parse("{albumartist|fallback:artist}").unwrap();
        let s = src(&[("artist", "Various")]);
        assert_eq!(t.render(&s).unwrap(), "Various");
    }

    #[test]
    fn fallback_misses_too() {
        let t = Template::parse("{albumartist|fallback:artist}").unwrap();
        let err = t.render(&src(&[])).unwrap_err();
        assert!(matches!(err, TemplateError::MissingVariable { .. }));
    }

    // ---- Width ----

    #[test]
    fn width_zero_pads_numeric() {
        let t = Template::parse("{n:03}").unwrap();
        assert_eq!(t.render(&src(&[("n", "7")])).unwrap(), "007");
    }

    #[test]
    fn width_truncates_strings() {
        let t = Template::parse("{title:5}").unwrap();
        assert_eq!(
            t.render(&src(&[("title", "Selected Ambient")])).unwrap(),
            "Selec"
        );
    }

    #[test]
    fn width_preserves_when_under() {
        let t = Template::parse("{n:02}").unwrap();
        assert_eq!(t.render(&src(&[("n", "42")])).unwrap(), "42");
    }

    // ---- Transformations ----

    #[test]
    fn sanitize_replaces_illegal() {
        let t = Template::parse("{title|sanitize}").unwrap();
        assert_eq!(
            t.render(&src(&[("title", "AC/DC: Back in Black?")]))
                .unwrap(),
            "AC_DC_ Back in Black_"
        );
    }

    #[test]
    fn ascii_folds_latin_diacritics() {
        let t = Template::parse("{artist|ascii}").unwrap();
        assert_eq!(t.render(&src(&[("artist", "Beyoncé")])).unwrap(), "Beyonce");
        assert_eq!(
            t.render(&src(&[("artist", "Mötley Crüe")])).unwrap(),
            "Motley Crue"
        );
    }

    #[test]
    fn lower_lowers() {
        let t = Template::parse("{x|lower}").unwrap();
        assert_eq!(t.render(&src(&[("x", "ABC")])).unwrap(), "abc");
    }

    #[test]
    fn upper_uppers() {
        let t = Template::parse("{x|upper}").unwrap();
        assert_eq!(t.render(&src(&[("x", "abc")])).unwrap(), "ABC");
    }

    #[test]
    fn title_case_works() {
        let t = Template::parse("{x|title}").unwrap();
        assert_eq!(
            t.render(&src(&[("x", "selected ambient works")])).unwrap(),
            "Selected Ambient Works"
        );
    }

    #[test]
    fn trim_strips_whitespace() {
        let t = Template::parse("{x|trim}").unwrap();
        assert_eq!(t.render(&src(&[("x", "  hi  ")])).unwrap(), "hi");
    }

    #[test]
    fn round_rounds_floats() {
        let t = Template::parse("{bpm|round}").unwrap();
        assert_eq!(t.render(&src(&[("bpm", "127.5")])).unwrap(), "128");
        assert_eq!(t.render(&src(&[("bpm", "126.2")])).unwrap(), "126");
    }

    #[test]
    fn round_preserves_non_numeric() {
        let t = Template::parse("{x|round}").unwrap();
        assert_eq!(
            t.render(&src(&[("x", "not-a-number")])).unwrap(),
            "not-a-number"
        );
    }

    #[test]
    fn max_truncates() {
        let t = Template::parse("{x|max:5}").unwrap();
        assert_eq!(t.render(&src(&[("x", "hello world")])).unwrap(), "hello");
    }

    // ---- Pipelines ----

    #[test]
    fn pipeline_applies_in_order() {
        let t = Template::parse("{x|trim|upper}").unwrap();
        assert_eq!(t.render(&src(&[("x", "  hi  ")])).unwrap(), "HI");
    }

    #[test]
    fn pipeline_with_fallback_and_transforms() {
        let t = Template::parse("{albumartist|fallback:artist|sanitize|lower}").unwrap();
        let s = src(&[("artist", "AC/DC: Live!")]);
        assert_eq!(t.render(&s).unwrap(), "ac_dc_ live!");
    }

    // ---- Realistic example ----

    #[test]
    fn realistic_meedya_dl_filename() {
        let t = Template::parse("{tracknumber:02} - {artist|sanitize} - {title|sanitize}.{ext}")
            .unwrap();
        let s = src(&[
            ("tracknumber", "3"),
            ("artist", "Aphex Twin"),
            ("title", "Selected Ambient Works"),
            ("ext", "flac"),
        ]);
        assert_eq!(
            t.render(&s).unwrap(),
            "03 - Aphex Twin - Selected Ambient Works.flac"
        );
    }

    #[test]
    fn realistic_meedya_manager_pathlike() {
        let t = Template::parse(
            "{albumartist|fallback:artist|sanitize}/{album|sanitize}/{tracknumber:02} - {title|sanitize}.{ext}",
        )
        .unwrap();
        let s = src(&[
            ("artist", "Aphex Twin"),
            ("album", "Selected Ambient Works 85-92"),
            ("tracknumber", "1"),
            ("title", "Xtal"),
            ("ext", "flac"),
        ]);
        assert_eq!(
            t.render(&s).unwrap(),
            "Aphex Twin/Selected Ambient Works 85-92/01 - Xtal.flac"
        );
    }

    // ---- TagSource alt impl ----

    #[test]
    fn static_str_hashmap_works_as_source() {
        let mut s: HashMap<&'static str, &'static str> = HashMap::new();
        s.insert("name", "world");
        let t = Template::parse("{name}").unwrap();
        assert_eq!(t.render(&s).unwrap(), "world");
    }
}
