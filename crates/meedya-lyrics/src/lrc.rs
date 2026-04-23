//! Minimal LRC parser and writer.
//!
//! Supports `[mm:ss]`, `[mm:ss.xx]`, and `[mm:ss.xxx]` timestamps. Multiple
//! leading timestamps on a single line are expanded to one [`SyncedLine`] per
//! timestamp. Metadata tags (`[ar:...]`, `[ti:...]`, etc.) are ignored.

use std::fmt::Write as _;
use std::time::Duration;

use crate::lyrics::SyncedLine;

pub fn parse(input: &str) -> Vec<SyncedLine> {
    let mut out = Vec::new();
    for raw in input.lines() {
        let mut rest = raw;
        let mut stamps: Vec<Duration> = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            match parse_timestamp(tag) {
                Some(d) => {
                    stamps.push(d);
                    rest = &stripped[end + 1..];
                }
                None => break,
            }
        }
        if stamps.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for at in stamps {
            out.push(SyncedLine {
                at,
                text: text.clone(),
            });
        }
    }
    out.sort_by_key(|l| l.at);
    out
}

fn parse_timestamp(tag: &str) -> Option<Duration> {
    let (mm, rest) = tag.split_once(':')?;
    let mins: u64 = mm.parse().ok()?;
    let (ss, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let secs: u64 = ss.parse().ok()?;
    if secs >= 60 {
        return None;
    }
    let frac_ms: u64 = if frac.is_empty() {
        0
    } else {
        let n: u64 = frac.parse().ok()?;
        match frac.len() {
            1 => n * 100,
            2 => n * 10,
            3 => n,
            len => n / 10u64.pow((len - 3) as u32),
        }
    };
    Some(Duration::from_millis(
        mins * 60_000 + secs * 1_000 + frac_ms,
    ))
}

pub fn write(lines: &[SyncedLine]) -> String {
    let mut out = String::new();
    for line in lines {
        let total_ms = line.at.as_millis() as u64;
        let mins = total_ms / 60_000;
        let secs = (total_ms / 1_000) % 60;
        let cs = (total_ms % 1_000) / 10;
        let _ = writeln!(out, "[{:02}:{:02}.{:02}]{}", mins, secs, cs, line.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lrc() {
        let src = "[ti:Hey]\n[00:12.34]Hello\n[01:02.50]World\n";
        let lines = parse(src);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].at, Duration::from_millis(12_340));
        assert_eq!(lines[0].text, "Hello");
        assert_eq!(lines[1].at, Duration::from_millis(62_500));
    }

    #[test]
    fn expands_multi_timestamp_lines() {
        let lines = parse("[00:01.00][00:05.00]repeat\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].at, Duration::from_millis(1_000));
        assert_eq!(lines[1].at, Duration::from_millis(5_000));
        assert_eq!(lines[0].text, "repeat");
        assert_eq!(lines[1].text, "repeat");
    }

    #[test]
    fn accepts_millisecond_precision() {
        let lines = parse("[00:00.123]x\n");
        assert_eq!(lines[0].at, Duration::from_millis(123));
    }

    #[test]
    fn round_trip() {
        let src = "[00:12.34]Hello\n[01:02.50]World\n";
        let parsed = parse(src);
        assert_eq!(write(&parsed), src);
    }

    #[test]
    fn skips_malformed_timestamps() {
        let lines = parse("[bad]skip\n[00:10.00]keep\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "keep");
    }
}
