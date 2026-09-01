// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Lucene/Solr query-string escaping for MusicBrainz search.
//
// MusicBrainz's search service (`musicbrainz.org/ws/2/<entity>/?query=...`)
// parses the `query` parameter with a Lucene/Solr query parser
// (https://musicbrainz.org/doc/MusicBrainz_API/Search — "The query field
// supports the full Lucene Search syntax"). Building that string by raw
// interpolation is unsafe in two distinct ways:
//
//   1. A bare, unquoted multi-word value only binds its FIRST token to the
//      field: `recording:Bohemian Rhapsody` parses as `recording:Bohemian`
//      AND a separate unfielded `Rhapsody` clause.
//   2. Characters such as `+ - && || ! ( ) { } [ ] ^ " ~ * ? : \ /` are
//      Lucene operators. Left unescaped they silently change how the query
//      is parsed — at best returning zero results, at worst a 400 from the
//      parser.
//
// This has always been true, but MusicBrainz's Solr 9 -> Solr 10 upgrade
// (SEARCH-764, 2026-11-30) tightens the parser, making previously-tolerated
// unescaped input more likely to fail outright. Everything in this module is
// valid against BOTH the current and the post-upgrade service — it produces
// standards-correct Lucene, which is what both parsers accept.
//
// # Which helper to use
//
// The two escaping regimes are genuinely different and are NOT
// interchangeable:
//
// | Context                       | Helper            | Escapes                     |
// |-------------------------------|-------------------|-----------------------------|
// | Bare / unquoted term          | [`escape_lucene`] | all Lucene special chars    |
// | Inside a double-quoted phrase | [`quote_phrase`]  | only `\` and `"`            |
// | A whole `field:"value"` clause| [`phrase_clause`] | only `\` and `"` (delegates)|
//
// Inside a quoted phrase Lucene treats `( ) : + - ?` etc. as literal text,
// so escaping them there is unnecessary and would make the escape
// backslashes part of the searched phrase. Outside a phrase they are
// operators and MUST be escaped — the MusicBrainz docs' own "AC/DC" example
// escapes the slash for exactly this reason.
//
// URL percent-encoding of the assembled query string is handled separately
// by `reqwest`'s `.query()` — this module only concerns itself with Lucene
// syntax, not URL syntax.
//
// This module is pure `std`, always compiled (no feature gate, no external
// dependencies) — a small, static text-escaping utility with no business
// logic, safe to depend on unconditionally.
//
// Consumers: `providers::musicbrainz`, `providers::isrc`, `providers::iswc`
// (all feature-gated), and downstream apps building their own MusicBrainz
// Lucene queries against this crate's `SearchQuery`.

/// Lucene special characters that require a backslash escape in a bare term.
///
/// Per the Lucene query syntax: `+ - && || ! ( ) { } [ ] ^ " ~ * ? : \ /`.
/// Listed individually here (rather than as `&&`/`||` pairs) because
/// escaping is done one character at a time.
const SPECIAL_CHARS: [char; 19] = [
    '+', '-', '!', '(', ')', '{', '}', '[', ']', '^', '"', '~', '*', '?', ':', '\\', '/', '&', '|',
];

/// Escape every Lucene special character in `value` with a backslash, for
/// embedding as a **bare (unquoted) term**.
///
/// Handles the single-character operators (`+ - ! ^ ~ * ? :`), grouping and
/// range syntax (`( ) { } [ ]`), the phrase delimiter (`"`), the escape
/// character itself (`\`), the regex/field-path separator (`/`), and the
/// boolean operator characters (`&` and `|` — so `&&`/`||` become
/// `\&\&`/`\|\|`).
///
/// This does **not** handle whitespace: a value containing spaces still
/// needs to be wrapped as a phrase for the query to treat it as one term,
/// because whitespace separates clauses regardless of escaping. Use
/// [`quote_phrase`] or [`phrase_clause`] for multi-word values.
///
/// Use this only for single-token values that must stay unquoted (e.g. a
/// normalised identifier). For anything user-supplied and multi-word, prefer
/// a phrase.
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

/// Wrap `value` as a Lucene phrase-query value: `"<escaped value>"`.
///
/// Escapes embedded backslashes and double quotes — backslash FIRST, so a
/// literal `\` is not double-escaped by the subsequent quote pass; doing it
/// in the other order would re-escape the backslashes just inserted for the
/// quote and corrupt the value.
///
/// Unlike [`escape_lucene`], other Lucene special characters (`+`, `(`, `:`,
/// etc.) are deliberately left as-is: Lucene does not interpret them
/// specially inside a quoted string, and escaping them would make the
/// backslashes part of the searched text.
///
/// Quoting also keeps multi-word values bound to their field as a single
/// phrase term.
///
/// The field qualifier stays **outside** the returned value:
/// ```
/// # use meedya_providers::quote_phrase;
/// let q = format!("recording:{}", quote_phrase("Where Is My Mind?"));
/// assert_eq!(q, r#"recording:"Where Is My Mind?""#);
/// ```
pub fn quote_phrase(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Build a complete, safely phrase-quoted Lucene field clause of the form
/// `field:"<escaped value>"`.
///
/// `field` is a developer-controlled literal (e.g. `"recording"`) and is
/// emitted verbatim; only `value` — arbitrary file- or user-derived input —
/// is escaped and quoted, via [`quote_phrase`].
///
/// This is the helper to reach for when building recording/release/artist
/// search clauses from tag data.
///
/// ```
/// # use meedya_providers::phrase_clause;
/// assert_eq!(
///     phrase_clause("artistname", "AC/DC"),
///     r#"artistname:"AC/DC""#
/// );
/// ```
pub fn phrase_clause(field: &str, value: &str) -> String {
    format!("{field}:{}", quote_phrase(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- escape_lucene: bare-term escaping ----

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
    fn escape_lucene_empty_string_returns_empty_string() {
        assert_eq!(escape_lucene(""), "");
    }

    // ---- quote_phrase: phrase-term escaping ----

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
    fn quote_phrase_backslash_before_quote_does_not_corrupt_the_quote_escape() {
        // Regression guard for escaping ORDER: a trailing backslash
        // immediately before a quote must become `\\` + `\"`, not `\"`
        // reinterpreted as an escaped backslash-quote pair.
        assert_eq!(quote_phrase("end\\\""), "\"end\\\\\\\"\"");
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

    #[test]
    fn quote_phrase_leaves_operator_chars_literal_inside_the_phrase() {
        // Inside a phrase these are literal text, so they must NOT be
        // backslash-escaped — escaping would search for the backslashes.
        assert_eq!(
            quote_phrase("What's Up? (Remix)"),
            r#""What's Up? (Remix)""#
        );
        assert_eq!(quote_phrase("cats && dogs"), r#""cats && dogs""#);
    }

    // ---- phrase_clause: full field clause ----

    #[test]
    fn phrase_clause_keeps_multi_word_value_fully_quoted() {
        assert_eq!(
            phrase_clause("recording", "Bohemian Rhapsody"),
            r#"recording:"Bohemian Rhapsody""#
        );
    }

    #[test]
    fn phrase_clause_slash_needs_no_escaping_inside_a_phrase() {
        assert_eq!(
            phrase_clause("artistname", "AC/DC"),
            r#"artistname:"AC/DC""#
        );
    }

    #[test]
    fn phrase_clause_neutralises_lucene_operator_characters() {
        assert_eq!(
            phrase_clause("recording", "What's Up? (Remix)"),
            r#"recording:"What's Up? (Remix)""#
        );
    }

    #[test]
    fn phrase_clause_embedded_quote_does_not_close_the_phrase_early() {
        assert_eq!(
            phrase_clause("recording", r#"Say "Hello""#),
            r#"recording:"Say \"Hello\"""#
        );
    }

    #[test]
    fn phrase_clause_empty_value_still_quotes() {
        assert_eq!(phrase_clause("recording", ""), r#"recording:"""#);
    }
}
