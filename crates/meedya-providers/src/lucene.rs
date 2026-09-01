// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Lucene query-string escaping helpers.
//
// MusicBrainz's `/recording`, `/work`, etc. search endpoints parse the
// `query` parameter as a Lucene query string
// (https://musicbrainz.org/doc/MusicBrainz_API/Search,
// https://lucene.apache.org/core/.../QueryParserSyntax.html). Building that
// string by raw interpolation is unsafe: a bare, unquoted multi-word value
// only binds its first token to the field (`recording:Bohemian Rhapsody`
// parses as `recording:Bohemian` AND an unfielded `Rhapsody` clause), and
// characters such as `: ( ) [ ] { } ~ * ? ^ " \` are Lucene operators when
// they appear outside of a quoted phrase.
//
// `lucene_phrase_clause` addresses this by producing a phrase-quoted,
// Lucene-escaped clause: wrapping a value in double quotes turns it into a
// single Lucene "phrase" term, which both keeps multi-word values attached
// to the field and neutralises Lucene's special characters within the
// value (they only carry syntactic meaning *outside* of a quoted phrase).
//
// URL percent-encoding of the assembled query string is handled separately
// by `reqwest`'s `.query()` — this module only concerns itself with Lucene
// syntax, not URL syntax.
//
// Mirrors the Lucene-escaping/phrase-quoting hardening in the sibling
// MeedyaConverter Swift repo (`MusicBrainzClient`, issue #493 Part A).

/// Escape a raw string for safe embedding inside a double-quoted Lucene
/// phrase.
///
/// Per Lucene's escaping rules, a backslash-escape sequence is introduced
/// with `\`, so any literal backslash in the value must be escaped to `\\`
/// *before* the phrase-terminating `"` character is escaped to `\"` — doing
/// it in the other order would re-escape the backslashes just inserted for
/// the quote, corrupting the value. Once quoted, Lucene treats every other
/// special character (`: ( ) [ ] { } ~ * ? ^` and friends) as a literal
/// part of the phrase rather than an operator, so no further escaping is
/// required.
///
/// - Parameter value: The raw, unescaped value (e.g. a track title, artist
///   name, or identifier) as supplied by the caller.
/// - Returns: `value` with `\` and `"` backslash-escaped.
pub fn lucene_escape(value: &str) -> String {
    // Order matters — see the doc comment above.
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Build a single, safely phrase-quoted Lucene field clause of the form
/// `field:"<escaped value>"`.
///
/// `field` is a developer-controlled literal (e.g. `"recording"`) and is
/// emitted verbatim; only `value` — arbitrary file- or user-derived input —
/// is escaped and quoted.
///
/// - Parameters:
///   - field: The Lucene/MusicBrainz field name, e.g. `"recording"`.
///   - value: The raw value to search for within that field.
/// - Returns: A Lucene clause, e.g. `recording:"Bohemian Rhapsody"`.
pub fn lucene_phrase_clause(field: &str, value: &str) -> String {
    format!("{field}:\"{}\"", lucene_escape(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_multi_word_value_stays_intact() {
        assert_eq!(lucene_escape("Bohemian Rhapsody"), "Bohemian Rhapsody");
    }

    #[test]
    fn phrase_clause_keeps_multi_word_value_fully_quoted() {
        assert_eq!(
            lucene_phrase_clause("recording", "Bohemian Rhapsody"),
            r#"recording:"Bohemian Rhapsody""#
        );
    }

    #[test]
    fn phrase_clause_handles_slash_without_escaping() {
        // `/` has no special meaning to Lucene's phrase parser; quoting
        // alone is sufficient.
        assert_eq!(
            lucene_phrase_clause("artistname", "AC/DC"),
            r#"artistname:"AC/DC""#
        );
    }

    #[test]
    fn phrase_clause_neutralises_lucene_operator_characters() {
        // `?`, `(`, `)` are Lucene operators outside of a quoted phrase;
        // once quoted they are literal.
        assert_eq!(
            lucene_phrase_clause("recording", "What's Up? (Remix)"),
            r#"recording:"What's Up? (Remix)""#
        );
    }

    #[test]
    fn escape_embedded_quote_is_backslash_escaped() {
        assert_eq!(lucene_escape(r#"Say "Hello""#), r#"Say \"Hello\""#);
    }

    #[test]
    fn phrase_clause_embedded_quote_does_not_close_the_phrase_early() {
        assert_eq!(
            lucene_phrase_clause("recording", r#"Say "Hello""#),
            r#"recording:"Say \"Hello\"""#
        );
    }

    #[test]
    fn escape_literal_backslash_is_doubled() {
        assert_eq!(lucene_escape(r"back\slash"), r"back\\slash");
    }

    #[test]
    fn escape_backslash_before_quote_does_not_corrupt_the_quote_escape() {
        // Regression guard for escaping order: a trailing backslash
        // immediately before a quote must become `\\` + `\"`, not `\"`
        // reinterpreted as an escaped backslash-quote pair.
        assert_eq!(lucene_escape("end\\\""), "end\\\\\\\"");
    }

    #[test]
    fn escape_boolean_operators_pass_through_unescaped() {
        // `&&` / `||` are Lucene boolean operators outside of a phrase but
        // carry no backslash/quote meaning, so `lucene_escape` (the
        // character-escaping primitive) leaves them untouched; it is the
        // phrase-quoting in `lucene_phrase_clause` that neutralises them.
        assert_eq!(lucene_escape("cats && dogs"), "cats && dogs");
        assert_eq!(
            lucene_phrase_clause("recording", "cats && dogs || birds"),
            r#"recording:"cats && dogs || birds""#
        );
    }

    #[test]
    fn escape_empty_string_returns_empty_string() {
        assert_eq!(lucene_escape(""), "");
    }

    #[test]
    fn phrase_clause_empty_value_still_quotes() {
        assert_eq!(lucene_phrase_clause("recording", ""), r#"recording:"""#);
    }
}
