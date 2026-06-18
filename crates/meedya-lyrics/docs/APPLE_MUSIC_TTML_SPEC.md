# Apple Music TTML Lyrics — Reverse-Engineered Specification

> # ⚠️ UNOFFICIAL DOCUMENTATION — read this first
>
> **Apple Music does not publish a specification for its TTML lyric
> format.** This document is a third-party, community
> reverse-engineering effort by the MeedyaSuite project. **Treat
> every claim here as an empirically-observed property, not an
> Apple-endorsed contract.** Apple could change any of the encoding
> details described here in any future release without notice.
>
> ## Why this document exists
>
> Two complementary use cases drove its creation:
>
> 1. **Downloading word/syllable-level lyrics from Apple Music** —
>    everything a MeedyaSuite (or third-party) downloader needs to
>    know to issue the right HTTP requests, authenticate properly,
>    and parse the TTML body. See [§1 — Endpoint contract](#1-where-this-ttml-comes-from).
>
> 2. **Composing new syllable-synced lyric documents in Apple's
>    TTML format** — for example, a lyric-database application
>    (iLyricsDB) that lets users author syllable-timed lyrics and
>    export them in the format Apple Music ships. The structural
>    rules — namespace declarations, the adjacent-spans-without-
>    whitespace syllable signal, background-vocal wrapping, song-
>    structure annotations — are all in this document.
>
> The MeedyaSuite project's sister project **iLyricsDB** consumes
> this document as the authoring-side reference for its syllable
> editor + Apple-compatible export.
>
> ## Status of each claim
>
> Every section is annotated with one of:
>
> - **VERIFIED** — observed directly in the canonical fixture
>   ([Closer_PrettyPrint.ttml][fixture-source]) AND in at least one
>   cross-source (the ITAM Enhancer userscript, an independent
>   reverse-engineering effort that consumes the same endpoints).
> - **ASSUMED** — inferred from a single source or from naming
>   conventions. May be wrong. Flagged explicitly inline so a future
>   verifier knows where to focus.
> - **UNKNOWN** — the canonical fixture doesn't exercise this case.
>   Listed in [§13 — Open questions](#13-open-questions--known-unknowns).
>
> When you find a claim that contradicts what you observe in the
> wild, **update this document first**, then implement. The spec is
> the source of truth, not the code.
>
> ## Provenance
>
> - Reference fixture: [closer-syllable-pretty.ttml][fixture]
>   (committed in this crate; trimmed from MeedyaDL's
>   [`.examplefiles/Syllable Level Synced Lyrics/Apple Music/1 - 01 - Closer_PrettyPrint.ttml`][fixture-source]).
> - **Endpoint audit** (2026-06-18): MeedyaDL workflow `w9zs75kdr` —
>   four parallel reverse-engineering agents cross-validated the HTTP
>   contract against the ITAM Enhancer userscript. See commit history
>   of [download_queue.rs][dq] and [apple_music_api.rs][ama] for the
>   consumer side.
> - **ITAM Enhancer userscript** (`skriptey.github.io/Userscripts/ITAMenhancer`)
>   — independent JavaScript implementation that fetches and renders
>   the same TTML in the Apple Music web player. Cross-referenced for
>   header semantics and the "richest TTML wins" fallback chain.
> - **Community resources not yet audited** — see [§17 — Community resources to cross-check](#17-community-resources-to-cross-check).

[itam]: https://skriptey.github.io/Userscripts/ITAMenhancer/ITAMenhancer.user.js
[fixture]: ../test-fixtures/closer-syllable-pretty.ttml
[fixture-source]: https://github.com/MWBMPartners/MeedyaDL/blob/main/.examplefiles/Syllable%20Level%20Synced%20Lyrics/Apple%20Music/1%20-%2001%20-%20Closer_PrettyPrint.ttml
[dq]: https://github.com/MWBMPartners/MeedyaDL/blob/main/src-tauri/src/services/download_queue.rs
[ama]: https://github.com/MWBMPartners/MeedyaDL/blob/main/src-tauri/src/services/apple_music_api.rs

---

## 1. Where this TTML comes from

Apple Music delivers lyrics as TTML XML strings inside a JSON
envelope returned by two song-relationship endpoints on the
**`amp-api.music.apple.com`** host:

```
GET https://amp-api.music.apple.com/v1/catalog/{country}/songs/{songId}/syllable-lyrics?extend=ttmlLocalizations
GET https://amp-api.music.apple.com/v1/catalog/{country}/songs/{songId}/lyrics?extend=ttmlLocalizations
```

| Endpoint | Granularity | Notes |
|---|---|---|
| `/syllable-lyrics` | Syllable, Word, or Line | The richest endpoint. Apple decides per track whether to emit syllable-level (long sustained vowels), word-level (typical pop song), or line-level (older catalog). |
| `/lyrics` | Line | Always line-level, never word/syllable. Used as a fallback when `/syllable-lyrics` returns 404. |

### Required headers

```
Authorization: Bearer {developerToken}
Origin: https://music.apple.com
Media-User-Token: {subscriberToken}        # mandatory for both endpoints
User-Agent: <a current browser UA string>  # see below
```

- `Authorization` **(VERIFIED)** — MusicKit JWT (ES256-signed). Three tiers of source covered in `apple_music_api.rs::resolve_premium_feature_token`.
- `Origin` **(VERIFIED)** — **mandatory**. `amp-api` rejects the CORS preflight without it. Easy to omit in a non-browser HTTP client.
- `Media-User-Token` **(VERIFIED)** — **mandatory**. Both endpoints are gated behind a logged-in Apple Music subscriber session; an anonymous catalog call returns "no related resources" and the body is empty. Note the spelling: **`Media-User-Token`**, not `Music-User-Token` (which is the colloquial name and may also be accepted today).
- `User-Agent` **(ASSUMED)** — Apple Music's anti-abuse infrastructure is widely reported to flag non-browser User-Agent strings (custom strings, library defaults, empty). Use a **current real browser UA** that matches what the Apple Music web player would send. A Safari-on-macOS UA is the most plausible match given the `Origin: https://music.apple.com` header. Rotate as Safari's version increments to avoid the UA growing stale.

  Recommended (as of mid-2026):
  ```
  Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15
  ```

  > **Note for downloader implementers.** Library-default UAs (`reqwest/0.11.x`, `python-requests/2.x`) are the single most likely reason a previously-working integration starts returning 403 or empty bodies. If your lyric fetch silently degrades, check your UA first.

### Required query parameter

```
?extend=ttmlLocalizations
```

**Mandatory** for both endpoints. Some songs ship the TTML body under
`attributes.ttmlLocalizations` instead of `attributes.ttml`; without
this extend flag those tracks return an empty `ttml` field and you
silently degrade to line-only.

### Response envelope

```json
{
  "data": [
    {
      "attributes": {
        "ttml": "<tt …>…</tt>",           // OR
        "ttmlLocalizations": "<tt …>…</tt>"
      }
    }
  ]
}
```

Read `attributes.ttml` first; fall back to
`attributes.ttmlLocalizations` as a string. Both are flat strings,
not language-keyed maps.

### Optional query parameter — `l={locale}`

```
?extend=ttmlLocalizations&l=en-US
```

Controls which translation/transliteration is served when multiple
`ttmlLocalizations` exist for a song. Omit to use the storefront's
default locale.

---

## 2. Document structure overview

```xml
<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml"
    xmlns:itunes="http://music.apple.com/lyric-ttml-internal"
    xmlns:ttm="http://www.w3.org/ns/ttml#metadata"
    itunes:timing="Word"
    xml:lang="en">
  <head>
    <metadata>
      <ttm:agent type="person" xml:id="v1"/>
      <iTunesMetadata xmlns="http://music.apple.com/lyric-ttml-internal"
                      leadingSilence="0.280">
        <translations/>
        <songwriters>
          <songwriter>Bernt Rune Stray</songwriter>
          …
        </songwriters>
        <audio lyricOffset="-0.271" role="spatial"/>
      </iTunesMetadata>
    </metadata>
  </head>
  <body dur="3:54.360">
    <div begin="7.516" end="15.514" itunes:songPart="Intro">
      <p begin="7.516" end="8.904" itunes:key="L1" ttm:agent="v1">
        <span begin="7.516" end="8.097">Clos</span><span begin="8.097" end="8.904">er</span>
      </p>
      …
    </div>
    …
  </body>
</tt>
```

The TTML has two top-level halves: a `<head>` block of song-wide
metadata (calibration values, songwriters, agents) and a `<body>`
block of timed lyric structure (`<div>` sections containing `<p>`
lines containing `<span>` words/syllables).

---

## 3. Namespaces

Apple TTML declares **three** namespaces, sometimes four:

| Prefix | URI | Purpose |
|---|---|---|
| (default) | `http://www.w3.org/ns/ttml` | W3C TTML 1.0 core |
| `itunes` | `http://music.apple.com/lyric-ttml-internal` | Apple-internal extensions (timing mode, song part, line ID, key) |
| `ttm` | `http://www.w3.org/ns/ttml#metadata` | W3C TTML metadata (agent, role) |
| `xml` | (built-in) | Standard XML attributes (`xml:lang`, `xml:id`) |

A second Apple namespace exists for files emitted by external
providers (lyric-provider portals submitting to Apple):

| Prefix | URI | When seen |
|---|---|---|
| `itunes` | `http://itunes.apple.com/lyric-ttml-extensions` | Provider-submitted TTML pre-ingest. Functionally interchangeable with the internal one for parsers; carry both URIs in any namespace-aware code path. |

> **Implementation note.** Parsers MUST resolve element/attribute
> matches by namespace URI lookup, NOT by literal prefix. External
> providers may alias the Apple namespace under a different prefix
> (e.g. `xmlns:apple="…lyric-ttml-internal"` then `apple:timing="Word"`).
> The classifier at [`lyricsfile_ttml_classify.rs`][classify]
> resolves both URIs via xmlns declaration lookup before matching.

[classify]: ../src/lyricsfile_ttml_classify.rs

---

## 4. The `<tt>` root element

### Attributes on `<tt>`

| Attribute | Type | Required | Example | Meaning |
|---|---|---|---|---|
| `xmlns` | URI | yes | `http://www.w3.org/ns/ttml` | TTML core namespace (default) |
| `xmlns:itunes` | URI | yes | `http://music.apple.com/lyric-ttml-internal` | Apple namespace |
| `xmlns:ttm` | URI | usually | `http://www.w3.org/ns/ttml#metadata` | TTML metadata namespace |
| `itunes:timing` | enum | usually | `Word`, `None` | See section 5 |
| `xml:lang` | ISO 639 code | usually | `en`, `ja`, `zh-Hans` | Document language hint |

### `itunes:timing` values

| Value | Meaning |
|---|---|
| `Word` | Document carries word-OR-syllable level timing. **Apple does NOT distinguish word from syllable at the attribute layer** — both share this value. Use structural detection (Section 5) to disambiguate. |
| `Line` | Document carries line-level timing only. Per-`<p>` `begin`/`end` attrs are present; per-`<span>` `begin` attrs are not. |
| `None` | Document is unsynced — lyric text only, no timing. Rare; usually means Apple has no synced lyric for this track. |

When the attribute is missing entirely, the document MAY still be
word-level — Apple sometimes omits `itunes:timing` on documents that
contain `<span begin>` children. Parsers MUST fall through to
structural detection.

---

## 5. Timing granularity — the THREE tiers

The single most important property of an Apple TTML document is its
**timing granularity**. The three tiers from richest to coarsest:

### Tier 1 — Syllable

Each `<p>` line contains a flat list of `<span begin="…" end="…">`
elements. **Within a single word**, the syllables that compose the
word are emitted as **adjacent `<span>` siblings with NO whitespace
between them**:

```xml
<p begin="7.516" end="8.904">
  <span begin="7.516" end="8.097">Clos</span><span begin="8.097" end="8.904">er</span>
</p>
```

Reads as "the word *Closer* is split into two timed syllables: *Clos*
(7.516–8.097s) and *er* (8.097–8.904s)".

### Tier 2 — Word

Same flat list of `<span begin>` elements, but **between words there
is whitespace** — typically a literal ASCII space inside a text node
between the two sibling elements:

```xml
<p begin="16.554" end="19.949">
  <span begin="16.554" end="17.058">Turn</span>
  <span begin="17.058" end="17.410">the</span>
  <span begin="17.410" end="17.727">lights</span>
</p>
```

(The whitespace between `</span>` and the next `<span begin>` can be
plain `\n  ` in pretty-printed output, or just ` ` in minified — both
count as a word-boundary signal.)

### Tier 3 — Line

The `<p>` carries plain text or only styling-`<span>`s (no `begin`
attribute on any child):

```xml
<p begin="00:00:01.000" end="00:00:03.500">Hello, it's me</p>
```

### Mixed granularity within one document

**The common case is MIXED.** Apple only splits the words that
genuinely sustain across multiple notes. A single `<p>` can carry
word-level spans for syllable-stable words AND syllable-level spans
for sustained ones:

```xml
<!-- L14: "To control me" — "To" and "me" are whole words, "contr"+"ol"
     is a syllable pair of the word "control". -->
<p begin="48.369" end="52.149">
  <span begin="48.369" end="48.905">To</span>
  <span begin="48.905" end="49.388">contr</span><span begin="49.388" end="51.372">ol</span>
  <span begin="51.372" end="52.149">me</span>
</p>
```

### How meedya-lyrics distinguishes the tiers

See [`lyricsfile_ttml_classify.rs`][classify]'s
`classify_ttml_granularity(ttml: &str) -> TtmlGranularity`. The
algorithm is a single quick-xml SAX walk tracking:

1. **`itunes:timing` attribute value** on `<tt>` — namespace-aware,
   resolves via xmlns lookup.
2. **Has timed span?** — `true` if any `<span begin>` is seen.
3. **Has gap-joined pair?** — `true` if two `<span begin>` siblings
   are seen with no whitespace-only text node between them.

The verdict logic:

| `gap_joined_pair` | `timed_span` | `itunes:timing` | → result |
|---|---|---|---|
| yes | (yes) | any | `Syllable` |
| no | yes | any | `Word` |
| no | no | `None` or absent | `Line` |
| (parse error) | | | `Unknown` |

The classifier descends ONE level into `<span ttm:role="x-bg">`
background-vocal wrappers (Section 9), so syllable-level background
vocals are detected too.

---

## 6. Time format

Apple uses **two interchangeable shapes** within the same file based
on magnitude:

| Format | Used for | Example |
|---|---|---|
| `SS.mmm` | Timestamps < 60 seconds | `7.516`, `48.905`, `59.591` |
| `M:SS.mmm` or `MM:SS.mmm` | Timestamps ≥ 60 seconds | `1:00.495`, `2:33.176` |
| `MM:SS.mmm` (no hours) on `body dur` | Total duration | `<body dur="3:54.360">` |

The pretty `HH:MM:SS.mmm` form **does not appear** in Apple files
(no track is long enough to require hours). Fractional precision is
consistently 3 decimal places (milliseconds).

Apple also accepts the W3C TTML 1.0 "clock-time" suffix form
(`12.5s`) in third-party-submitted TTML, but Apple's own emission
never uses it.

### Parser implementation

Parsers must split on `:` and treat the fields as **right-anchored**
(seconds always present, minutes optional, hours optional). The
canonical parser is in [`lyricsfile_ttml.rs`][ttml]'s
`parse_ttml_time`. Test fixtures cover all five recognised shapes:
`HH:MM:SS.mmm`, `HH:MM:SS.cc`, `MM:SS.mmm`, `SS.mmm`, and the
`<number>s` clock-time suffix.

[ttml]: ../src/lyricsfile_ttml.rs

---

## 7. The `<head>` block — document metadata

### `<ttm:agent>` — speakers

Declares vocalists. Single-speaker tracks have one agent (`xml:id="v1"`);
duets have two or more.

```xml
<ttm:agent type="person" xml:id="v1"/>
<ttm:agent type="person" xml:id="v2"/>     <!-- duet partner -->
<ttm:agent type="group" xml:id="g1"/>      <!-- backing chorus -->
```

| Attribute | Values |
|---|---|
| `type` | `person`, `group` |
| `xml:id` | Stable handle referenced by each `<p>`'s `ttm:agent` attribute |

The Closer fixture only declares one agent and isn't a duet, so the
duet code path is **not exercised by the canonical fixture**. See
Section 13 (Open Questions) for what's unverified.

### `<iTunesMetadata>` — Apple-specific calibration & credits

Lives in the Apple namespace. The element itself can carry one or
two attributes:

| Attribute | Type | Sample | Meaning |
|---|---|---|---|
| `leadingSilence` | seconds (float) | `"0.280"` | **AAC encoder pre-roll silence.** Handled at the audio-decoder layer (CoreAudio gapless metadata, libavcodec priming sample). NOT a lyric calibration value — DO NOT lift into `Lyricsfile.metadata.offset_ms`. |
| `xmlns` | URI | `"http://music.apple.com/lyric-ttml-internal"` | Often (redundantly) restates the Apple namespace inside its own element. |

Children:

#### `<translations/>` — usually empty

The fixture has `<translations/>`. Multilingual songs (Apple Music's
"Lyrics in" feature) populate this with localized variants. Schema
not yet documented here — when a multilingual fixture surfaces we
amend.

#### `<songwriters><songwriter>…</songwriter>…</songwriters>`

Flat list of credited songwriters in document order. The Closer
fixture has 5. Not yet lifted into `Lyricsfile.metadata` — see
Section 13.

#### `<audio lyricOffset="…" role="…"/>` — self-closed

```xml
<audio lyricOffset="-0.271" role="spatial"/>
```

| Attribute | Type | Meaning |
|---|---|---|
| `lyricOffset` | seconds (signed float) | **Lyric timing calibration.** Lifted into `Lyricsfile.metadata.offset_ms` (preserving sign — see caveat in Section 9.4). |
| `role` | enum | Audio master role. Seen: `"spatial"` (Dolby Atmos master), `"stereo"` (assumed). |

The `role="spatial"` flag suggests the lyric was authored against a
Dolby Atmos master — Apple sometimes ships different `lyricOffset`
values for Atmos vs stereo masters of the same song. This isn't
exposed via the API (the user always gets one version).

---

## 8. The `<body>` block — timed lyric structure

### `<body>` attributes

| Attribute | Value | Meaning |
|---|---|---|
| `dur` | clock-time (`M:SS.mmm`) | Total track duration. Useful for player playhead alignment. |

### `<div>` sections — song structure

The `<body>` contains a flat list of `<div>` elements, each
representing a song-structural section:

```xml
<div begin="7.516" end="15.514" itunes:songPart="Intro">…</div>
<div begin="16.554" end="31.112" itunes:songPart="Verse">…</div>
<div begin="52.759" end="59.591" itunes:songPart="PreChorus">…</div>
<div begin="1:00.495" end="1:14.969" itunes:songPart="Chorus">…</div>
```

| Attribute | Type | Meaning |
|---|---|---|
| `begin` | TTML time | Section start (matches first child `<p>`'s `begin`). |
| `end` | TTML time | Section end (matches last child `<p>`'s `end`). |
| `itunes:songPart` | enum | See below. |

#### `itunes:songPart` recognised values

Observed in the Closer fixture:

- `Intro`
- `Verse`
- `PreChorus`
- `Chorus`
- `Bridge`
- `Outro`

Other values plausible per Apple's broader catalog: `Refrain`,
`Solo`, `Interlude`, `Hook`, `PostChorus`. Not yet confirmed.

### `<p>` lines

Each line is a `<p>` with timing + identifier metadata + agent
binding:

```xml
<p begin="7.516" end="8.904"
   itunes:key="L1"
   ttm:agent="v1">
  <span begin="7.516" end="8.097">Clos</span><span begin="8.097" end="8.904">er</span>
</p>
```

| Attribute | Type | Meaning |
|---|---|---|
| `begin` | TTML time | Line start. Required for the parser to emit a `LyricsfileLine`. |
| `end` | TTML time | Line end. Optional (parsers can backfill from the next line's `begin`). |
| `itunes:key` | `L{n}` | Stable line identifier (`L1`, `L2`, …, `L63` in Closer). Monotonically incrementing across the document. Useful for diffing across Apple lyric revisions, but not currently lifted into the Lyricsfile schema. |
| `ttm:agent` | reference to `<ttm:agent xml:id>` | Speaker for this line. Single-speaker songs always have `v1`. |

### `<span>` words/syllables

Inside a `<p>`, the `<span>` elements carry the per-word or
per-syllable timing. Each timed `<span>` has only `begin` and `end`
attributes — no `xml:id`, no agent override, no styling.

```xml
<span begin="17.058" end="17.410">the</span>
```

| Attribute | Type | Required | Meaning |
|---|---|---|---|
| `begin` | TTML time | yes for timed | When this fragment starts. |
| `end` | TTML time | yes for timed | When this fragment ends. |

A `<span>` **without** a `begin` attribute is a non-timed wrapper —
either a styling artefact or a background-vocal container (Section 9).

### Gapless guarantee within a word

`<span begin/end>` of a syllable pair is **gapless**: the `end` time
of syllable N is byte-identical to the `begin` time of syllable N+1
(see `Clos`/`er` example, both `8.097`). This is what makes the
"adjacent with no whitespace text node" structural signal reliable —
Apple's authoring tools never insert a deliberate gap mid-word.

---

## 9. Background vocals — `ttm:role="x-bg"`

Background-vocal sections are wrapped in an OUTER non-timed
`<span ttm:role="x-bg">` containing INNER timed `<span begin>` elements:

```xml
<p begin="1:53.630" end="1:58.313" itunes:key="L31" ttm:agent="v1">
  <span begin="1:53.630" end="1:54.187">Come</span>
  <span begin="1:54.187" end="1:55.121">clos</span><span begin="1:55.121" end="1:56.818">er</span>
  <span ttm:role="x-bg"><span begin="1:56.181" end="1:57.086">(Clos</span><span begin="1:57.086" end="1:58.313">er)</span></span>
</p>
```

### Key properties

- Outer wrapper has **no** `begin` attribute.
- Outer wrapper carries `ttm:role="x-bg"` (no other roles observed; `x-` prefix is TTML's extension convention).
- Inner spans follow the same word/syllable convention (whitespace = word boundary; gap-joined = syllable continuation).
- Lead vocals carry `ttm:agent="v1"` on the parent `<p>`. Background spans **inherit** that agent — the inner spans do NOT carry their own `ttm:agent` attribute.
- Background span timestamps can **overlap** lead-vocal timestamps (typical: backgrounds echo the previous lead phrase, so they start before the lead phrase ends).
- Punctuation: backgrounds typically wrap their content in parentheses `(…)` as plain text inside the spans.

### Parsing implementation

The classifier in [`lyricsfile_ttml_classify.rs`][classify] descends
ONE level into x-bg wrappers — adjacent timed spans inside the
wrapper still count as syllable pairs (of that background vocal
word). The from_ttml converter currently treats x-bg inner spans
identically to lead-vocal spans (no separate agent/role tracking),
which captures the timing but loses the lead-vs-background
distinction. See Section 13 for the planned refinement.

---

## 10. Apple-specific attribute deep-dive

### 10.1 `itunes:timing` on `<tt>`

Already covered in Section 5. Bears repeating: **`Word` does NOT
distinguish word-level from syllable-level.** Apple uses the same
value for both. Structural detection is mandatory.

### 10.2 `itunes:songPart` on `<div>`

Already covered in Section 8. Recognised values: `Intro`, `Verse`,
`PreChorus`, `Chorus`, `Bridge`, `Outro`. Future MeedyaSuite UI
features (chapter-style jump-to-section playback) could consume this.

### 10.3 `itunes:key` on `<p>`

Stable per-line identifier of form `L{n}` where n monotonically
increments across the document. Useful for:

- Diffing Apple lyric revisions across catalog updates.
- Implementing per-line UI bookmarks that survive a re-fetch.

Not lifted into the Lyricsfile schema yet. If a consumer needs it,
add an optional `apple_line_key: Option<String>` field on
`LyricsfileLine` — additive schema change.

### 10.4 `leadingSilence` on `<iTunesMetadata>`

**Not a lyric concern.** This is the AAC encoder priming-sample
duration that the audio decoder strips for gapless playback. Lifting
into `Lyricsfile.metadata.offset_ms` would double-correct on any
player that already honours the AAC priming sample.

**Decision:** ignored.

### 10.5 `lyricOffset` on `<audio>` inside `<iTunesMetadata>`

**The lyric calibration offset.** Lifted into
`Lyricsfile.metadata.offset_ms` by
[`lyricsfile_ttml.rs::read_lyric_offset_attr`][ttml-offset].

[ttml-offset]: ../src/lyricsfile_ttml.rs

#### Sign convention — current assumption (unverified)

| Apple wire | Lyricsfile field | LRC export |
|---|---|---|
| `lyricOffset="-0.271"` | `offset_ms = Some(-271)` | `[offset:-271]` |
| `lyricOffset="0.500"` | `offset_ms = Some(500)` | `[offset:500]` |

The current implementation **preserves Apple's sign verbatim** — no
inversion. This matches the LRC convention "positive shifts lyrics
LATER, negative shifts EARLIER" *only if* Apple's `lyricOffset` uses
the same convention.

That equivalence is **assumed, not verified**. See MeedyaSuite-core
issue #61 for the verification plan. If a real-player A/B test shows
the sign is inverted in player output, the fix is a one-line
negation inside `read_lyric_offset_attr` (and the corresponding test
assertions flip).

### 10.6 `role` on `<audio>`

Audio master role. Observed values:

- `"spatial"` — Dolby Atmos master (Closer fixture)
- `"stereo"` — assumed; not yet seen in a fixture

The user always gets one version of the TTML per request — Apple
doesn't expose per-master TTML variants through the API. The role
attribute is informational only.

---

## 11. Punctuation handling

Punctuation is **attached to the last syllable of its word** as
plain text inside the `<span>`. Apple does not externalise
punctuation into separate spans.

### Examples from the Closer fixture

| Phrase | Encoding |
|---|---|
| `Woo!` | `<span begin="15.029" end="15.514">Woo!</span>` — exclamation mark inline |
| `don't` | `<span begin="…">don't</span>` — single span, apostrophe inline |
| `stop,` | `<span begin="…">stop,</span>` — comma inline |
| `escape` | `<span begin="…">escape</span>` — no surrounding spans |
| `(Closer)` | `<span begin="…">(Clos</span><span begin="…">er)</span>` — outer parens belong to the syllables they touch |

### Implication for parsers

When reconstructing the line text from word-level spans, **use the
literal inter-span text** (whitespace) as the separator, not a
`" "` you inject yourself. Apple's whitespace is the source of truth:

```rust
// WRONG: injects spaces that may differ from Apple's encoding
let text = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");

// RIGHT: track literal text between spans and use it as the separator
//        (or accept that " "-joining is a best-effort approximation that
//        may differ from Apple's display string at sub-character precision).
```

The current `from_ttml` implementation uses " "-joining as a
best-effort. For lyric DISPLAY this is fine; for lossless
round-tripping consider preserving the raw text. The Lyricsfile
schema doesn't currently capture the inter-word whitespace
explicitly.

---

## 12. Implementation reference — how meedya-lyrics processes each layer

| TTML concept | meedya-lyrics handling | Source |
|---|---|---|
| Namespace resolution | xmlns-declaration lookup, not literal prefix match | `lyricsfile_ttml_classify.rs::find_itunes_timing_value` |
| `itunes:timing` attribute | Read, but **not trusted alone** — structural pass dominates | `classify_ttml_granularity` pass 1 |
| Granularity detection | Single quick-xml SAX walk with whitespace-text-node tracking | `classify_ttml_granularity` |
| `<head>` `<ttm:agent>` | Not yet lifted into the schema | — |
| `<head>` `<iTunesMetadata leadingSilence>` | Ignored (audio-decoder concern) | — |
| `<head>` `<iTunesMetadata songwriters>` | Not yet lifted | — |
| `<head>` `<iTunesMetadata audio lyricOffset>` | → `metadata.offset_ms` (sign preserved) | `lyricsfile_ttml.rs::read_lyric_offset_attr` |
| `<body dur>` | Not yet lifted (would feed `metadata.duration_ms`) | — |
| `<div itunes:songPart>` | Not yet lifted | — |
| `<p itunes:key>` | Not yet lifted | — |
| `<p ttm:agent>` | Not yet lifted | — |
| `<p>` line emit | One `LyricsfileLine` per `<p>` with `begin` attr | `Lyricsfile::from_ttml` |
| `<span begin>` word | One `LyricsfileWord` per timed span; gap-joined siblings merge into syllables | `Lyricsfile::from_ttml` syllable grouping branch |
| `<span ttm:role="x-bg">` background | Inner timed spans descended one level — treated as additional words/syllables of the same `<p>` (no agent/role distinction yet) | `Lyricsfile::from_ttml` |
| Time format `SS.mmm` / `M:SS.mmm` / `HH:MM:SS.mmm` / `<n>s` | All five forms accepted | `parse_ttml_time` |
| XML entities (`&apos;`, `&amp;`) | Decoded via `quick-xml`'s `unescape` | `Event::Text` handler |

---

## 13. Open questions / known unknowns

The fixture is one song. These properties are documented from
limited evidence and need additional fixtures to confirm:

1. **`lyricOffset` sign convention.** Unverified. Tracked as MeedyaSuite-core #61. Need ~5 real-player A/B tests against songs with non-zero offsets.
2. **Duet TTML structure.** `<ttm:agent>` schema supports duets via multiple `xml:id`s and per-`<p>` `ttm:agent` references, but the canonical fixture is single-speaker. Need a duet fixture to confirm the per-`<p>` agent switching works.
3. **`<translations/>` schema for multilingual songs.** Empty in the canonical fixture. Apple's "Lyrics in" multilingual feature populates this — schema not yet documented.
4. **Other `role` values on `<audio>`.** Only `spatial` seen. `stereo` and a possible `binaural` are assumed but not confirmed.
5. **Other `itunes:songPart` values.** Six values observed; `Refrain`, `Solo`, `Interlude`, `Hook`, `PostChorus` are plausible but unconfirmed.
6. **Punctuation outside the last syllable.** The fixture only shows punctuation attached to the last syllable of the word it terminates. Older catalog entries may use different conventions (e.g. comma inline at the start of the next span). Not yet seen.
7. **CJK-language whitespace.** The canonical fixture is English. CJK lyrics may encode word boundaries with zero-width or non-breaking whitespace (U+00A0 NBSP, U+200B ZWSP). The current structural-pair check treats these as not-whitespace, which may misclassify CJK syllable lyrics. Defer until a CJK fixture surfaces; the new MeedyaDL test-connection IPC (#936) is planned to log `xml:lang` for collected diagnostics.
8. **`itunes:key` uniqueness across translations.** If the `<translations/>` block carries variant TTML, are `L1`…`Ln` identifiers stable across translations? Unknown.
9. **`<body dur>` precision.** Always `M:SS.mmm` in the fixture. Long tracks (>1 hour) presumably switch to `H:MM:SS.mmm` — not yet seen.
10. **Maximum syllables per word.** Closer's longest seen is 2 (`Clos`+`er`). Apple's authoring tools presumably support more (`be`+`au`+`ti`+`ful` = 4) but the canonical fixture doesn't exercise this. The `from_ttml` implementation handles arbitrary counts via the promote-then-append branch.

---

## 14. Reference fixtures

Committed in `crates/meedya-lyrics/test-fixtures/`:

| File | Source | Use |
|---|---|---|
| `closer-syllable-pretty.ttml` | Trimmed copy of Apple's `/syllable-lyrics` for *Closer* — Ne-Yo. Pretty-printed (Apple's raw output is minified). | Canonical anchor for everything in this document. Inputs to the classifier + from_ttml syllable grouping + lyricOffset extraction tests. |
| `word-only-minified.ttml` | Synthesised minified word-only TTML — Closer's Verse line with whitespace between structural elements stripped (preserves literal spaces between spans). | Adversarial fixture pinning that minified word-only TTML does NOT false-positive to Syllable. |

Future fixtures to collect (one per open question above):

- A duet (e.g. Mariah Carey ft Boyz II Men "One Sweet Day").
- A multilingual song with `<translations>` populated (Apple's Latin / K-pop / J-pop catalog).
- A CJK song (Japanese, Korean, or Chinese).
- A song with a non-zero `lyricOffset` of either sign, paired with an A/B player test result so the sign convention can be pinned.

---

## 15. Cross-format conversion summary

How Apple TTML's data flows through meedya-lyrics into the five
supported export targets:

| Apple TTML | → Lyricsfile | → LRC | → Enhanced LRC | → SRT | → WebVTT | → ASS |
|---|---|---|---|---|---|---|
| `xml:lang` | `metadata.language` | (dropped) | (dropped) | (dropped) | (dropped) | (dropped) |
| `lyricOffset` | `metadata.offset_ms` | `[offset:NNN]` | `[offset:NNN]` | (dropped) | (dropped) | (dropped) |
| `<body dur>` | (not yet lifted) | — | — | — | — | — |
| `<div itunes:songPart>` | (not yet lifted) | — | — | — | — | — |
| `<p begin/end>` | `LyricsfileLine.start_ms/end_ms` | line timestamp | line timestamp | cue range | cue range | dialogue range |
| `<p itunes:key>` | (not yet lifted) | — | — | — | — | — |
| `<p ttm:agent>` | (not yet lifted) | — | — | — | — | — |
| `<span begin>` word | `LyricsfileWord` | (collapsed) | `<mm:ss.xx>word` | (collapsed) | (collapsed) | (collapsed) |
| `<span begin>` syllable | `LyricsfileSyllable` | (collapsed via word) | (collapsed via word — Syllable Enhanced LRC follow-up) | (collapsed) | (collapsed) | (collapsed) |
| `<span ttm:role="x-bg">` | (flattened with lead vocals) | mixed | mixed | mixed | mixed | mixed |
| Punctuation inside spans | preserved in `text` | preserved | preserved | preserved | preserved | preserved |

---

## 16. Document history

| Date | Author | Change |
|---|---|---|
| 2026-06-18 | Lance / Claude | Initial spec — committed alongside MeedyaSuite-core #60 (syllable schema + classifier) and #60 follow-ups B (`lyricOffset` extraction) + C (LRC `[offset:]` round-trip). |

When you discover a quirk that contradicts what's here — or find a
fixture that exercises one of the open questions — **update this
file first**, in the same commit that adds the test for the new
behaviour. The spec is the source of truth, not the code.

---

## 17. Community resources to cross-check

This document was assembled from the canonical Closer fixture and
the ITAM Enhancer userscript. **Several other community sources have
done parallel reverse-engineering work** on the Apple Music TTML
format. A future contributor should survey these and reconcile any
conflicts with the claims above:

- **GAMDL** (`glomatico/gamdl`, Python) — MeedyaDL's downstream
  CLI subprocess for Apple Music. Its TTML parsing logic in
  `gamdl/interface/song.py::_get_lyrics_synced` and the
  `enhanced_lyrics_helpers` module is one of the most complete
  open-source consumers of this format. Cross-check the timing
  format parser, syllable detection, and background-vocal handling.
- **LRCGET** (`tranxuanthang/lrcget`) — consumes Enhanced LRC
  derived from this format (via this crate's exports). Its renderer
  is the reference for our `[offset:]` round-trip sign convention.
- **lyric-providers** repositories on GitHub — search GitHub for
  `apple-music ttml` and filter to repos with >50 stars; several
  reverse-engineering blogs and notebooks have annotated specific
  attributes that aren't documented here.
- **Apple Developer Forums + community Discord servers** — most
  detail about edge cases (multilingual `<translations>`, duet
  agents, Atmos-vs-stereo `lyricOffset` variants) lives in informal
  threads. Worth a half-day archaeology pass before a major
  revision of this spec.
- **Web Archive snapshots of `developer.apple.com/musickit/`** —
  Apple has occasionally published partial schema docs that were
  later withdrawn. Search the archive for any leaked attribute
  reference.
- **`syllab-ttml` and similar npm packages** — community JavaScript
  parsers exist on npm; they're typically thin wrappers but
  occasionally annotate quirks not seen in our fixture.

When you do the archaeology pass, **annotate findings into this
document with a `(VERIFIED via X)`, `(CORRECTED via X)`, or
`(STILL UNKNOWN — X was inconclusive)` tag** rather than overwriting
the existing claim silently. The document's value is partly in
showing which claims have been cross-validated and which haven't.

---

## Appendix A — Producing a Word-document copy

This file is plain CommonMark markdown. To convert to a `.docx`:

```bash
pandoc --from=gfm --to=docx \
       --output=APPLE_MUSIC_TTML_SPEC.docx \
       crates/meedya-lyrics/docs/APPLE_MUSIC_TTML_SPEC.md
```

The Word output preserves the tables, code blocks, and heading
hierarchy. Pair with a Word template (`--reference-doc=…`) if you
need MWBM Partners letterhead styling.

## Appendix B — External references

- [W3C TTML 1.0 (Timed Text Markup Language)](https://www.w3.org/TR/ttml1/) — base spec Apple extends.
- [W3C TTML2](https://www.w3.org/TR/ttml2/) — successor spec; some Apple constructs match TTML2 even though the namespace declares TTML 1.0.
- [ITAM Enhancer userscript][itam] — independent consumer of the same endpoints; cross-reference for header semantics.
- [LRCGET v2.0.0 Lyricsfile spec](https://github.com/tranxuanthang/lrcget/releases/tag/2.0.0) — the YAML format meedya-lyrics serialises into.
- [LRC `[offset:]` tag behaviour](https://en.wikipedia.org/wiki/LRC_(file_format)) — sign convention for the calibration tag.
- MeedyaDL `apple_music_api.rs::fetch_syllable_lyrics` — Rust implementation of the HTTP layer described in §1.
- MeedyaSuite-core `lyricsfile_ttml.rs` and `lyricsfile_ttml_classify.rs` — Rust implementation of the parsing layer.

[itam]: https://skriptey.github.io/Userscripts/ITAMenhancer/ITAMenhancer.user.js
