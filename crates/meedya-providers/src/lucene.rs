// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Lucene/Solr query escaping for MusicBrainz search.
//
// MusicBrainz's search service (musicbrainz.org/ws/2/*?query=...) is backed
// by a Lucene/Solr query parser. Any user-supplied value (a track title, an
// artist name, a free-text search term) that reaches that parser unescaped
// can be misinterpreted as query syntax — a stray `"`, `(`, or `:` changes
// how the query is parsed, at best returning zero results and at worst a
// 400 from the parser. This has always been true, but MusicBrainz's planned
// Solr 9 -> 10 upgrade (2026-11-30) tightens the parser, making previously
// "tolerated" unescaped input more likely to fail outright.
//
// This module is pure `std`, always compiled (no feature gate, no external
// dependencies) — it is a small, static text-escaping utility with no
// business logic, safe to depend on unconditionally.
//
// Consumers: `providers::musicbrainz`, `providers::isrc`, `providers::iswc`
// (all feature-gated), and downstream apps building their own MusicBrainz
// Lucene queries against this crate's `SearchQuery`.

/// Lucene special characters that require a backslash escape.
///
/// Per the Lucene query syntax: `+ - && || ! ( ) { } [ ] ^ " ~ * ? : \ /`.
/// Listed individually here (rather than as `&&`/`||` pairs) because
/// escaping is done one character at a time.
const SPECIAL_CHARS: [char; 19] = [
    '+', '-', '!', '(', ')', '{', '}', '[', ']', '^', '"', '~', '*', '?', ':', '\\', '/', '&', '|',
];

/// Escape every Lucene special character in `value` with a backslash.
///
/// Handles the single-character operators (`+ - ! ^ ~ * ? :`), grouping and
/// range syntax (`( ) { } [ ]`), the phrase delimiter (`"`), the escape
/// character itself (`\`), the field-path separator (`/`), and the boolean
/// operator characters (`&` and `|` — so `&&`/`||` become `\&\&`/`\|\|`).
///
/// This does **not** handle whitespace or field-scoping — a value with
/// spaces still needs to be wrapped as a phrase for the query to treat it
/// as one term. Use [`quote_phrase`] for that; the field qualifier itself
/// (e.g. `recording:`) stays outside the quoted value.
pub fn escape_lucene(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if SPECIAL_CHARS.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Wrap `value` as a Lucene phrase query value.
///
/// Escapes embedded backslashes and double quotes (backslash first, so a
/// literal `\` is not double-escaped by the subsequent quote pass), then
/// wraps the result in double quotes. Unlike [`escape_lucene`], other Lucene
/// special characters (`+`, `(`, `:`, etc.) are left as-is inside the phrase
/// — Lucene does not interpret them specially within a quoted string.
///
/// The field qualifier stays **outside** the returned value, e.g.:
/// ```
/// # use meedya_providers::quote_phrase;
/// let q = format!("recording:{}", quote_phrase("Where Is My Mind?"));
/// assert_eq!(q, r#"recording:"Where Is My Mind?""#);
/// ```
pub fn quote_phrase(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_lucene_leaves_clean_string_unchanged() {
        assert_eq!(escape_lucene("Comfortably Numb"), "Comfortably Numb");
    }

    #[test]
    fn escape_lucene_grouping_and_range_chars() {
        assert_eq!(escape_lucene("(a) [b] {c}"), "\\(a\\) \\[b\\] \\{c\\}");
    }

    #[test]
    fn escape_lucene_single_char_operators() {
        assert_eq!(
            escape_lucene("+ - ! ^ ~ * ? :"),
            "\\+ \\- \\! \\^ \\~ \\* \\? \\:"
        );
    }

    #[test]
    fn escape_lucene_quote_backslash_and_slash() {
        assert_eq!(
            escape_lucene(r#"say "hi" \ AC/DC"#),
            r#"say \"hi\" \\ AC\/DC"#
        );
    }

    #[test]
    fn escape_lucene_double_ampersand() {
        assert_eq!(escape_lucene("Rock && Roll"), "Rock \\&\\& Roll");
    }

    #[test]
    fn escape_lucene_double_pipe() {
        assert_eq!(escape_lucene("A || B"), "A \\|\\| B");
    }

    #[test]
    fn quote_phrase_plain() {
        assert_eq!(quote_phrase("Back in Black"), "\"Back in Black\"");
    }

    #[test]
    fn quote_phrase_embedded_quote() {
        assert_eq!(quote_phrase(r#"Say "Hello""#), r#""Say \"Hello\"""#);
    }

    #[test]
    fn quote_phrase_embedded_backslash() {
        assert_eq!(quote_phrase(r"back\slash"), r#""back\\slash""#);
    }

    #[test]
    fn quote_phrase_empty() {
        assert_eq!(quote_phrase(""), "\"\"");
    }

    #[test]
    fn quote_phrase_multi_word_is_not_split() {
        let phrase = quote_phrase("Nine in the Afternoon");
        assert_eq!(phrase, "\"Nine in the Afternoon\"");
        assert_eq!(phrase.matches(' ').count(), 3);
    }
}
