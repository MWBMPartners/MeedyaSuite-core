#!/usr/bin/env python3
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# Makes the small, real MP3 and WAV files the `tag_io` tests read for the
# refusal of language frames in a file with more than one ID3 tag.
# ====================================================================
#
# Why real files: lofty finds an MP3's tags, and a WAV file's ID3 chunks,
# by walking the file the way real players do, so the tags have to sit in
# front of real audio. The audio is a tenth of a second of a tone made by
# ffmpeg (no tags of its own); the ID3v2 tags are built byte by byte here,
# because no tagging tool will write two tags one after another, or two
# language frames in one tag, on purpose. These are the shapes the stand-in
# review of revision 9 made to show the fault (finding M2).
#
# Made with ffmpeg 9.0.1. Run from this folder:
#     python3 make_id3_fixtures.py

import os
import struct
import subprocess


def run(*args):
    subprocess.run(list(args), check=True)


def synchsafe(n):
    """A tag or ID3v2.4 frame size: four bytes of seven bits each."""
    return bytes([(n >> 21) & 0x7F, (n >> 14) & 0x7F, (n >> 7) & 0x7F, n & 0x7F])


def frame(name, text):
    """An ID3v2.4 text frame, UTF-8 (encoding byte 3)."""
    body = b"\x03" + text
    return name + synchsafe(len(body)) + b"\x00\x00" + body


def tag(frames, padding=0):
    """An ID3v2.4 tag holding `frames`, then `padding` zero bytes."""
    body = b"".join(frames) + b"\x00" * padding
    return b"ID3\x04\x00\x00" + synchsafe(len(body)) + body


run("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=44100:duration=0.1",
    "-c:a", "libmp3lame", "-b:a", "128k", "-id3v2_version", "0", "-write_id3v1", "0",
    "-write_xing", "0", "-y", "tone.mp3")
AUDIO = open("tone.mp3", "rb").read()
assert AUDIO[:3] != b"ID3", "ffmpeg wrote a tag"
os.remove("tone.mp3")

TITLE_ONLY = tag([frame(b"TIT2", b"Title One")])
ONE_LANGUAGE = tag([frame(b"TLAN", b"eng")])
SPLIT = [frame(b"TLAN", b"eng"), frame(b"TLAN", b"fra")]

# Two tags one after another; the first has no language, the second one
# language frame (`eng`).
open("two-tags-lang-in-second.mp3", "wb").write(TITLE_ONLY + ONE_LANGUAGE + AUDIO)
# The same, the second tag holding the languages in two frames.
open("two-tags-split-in-second.mp3", "wb").write(TITLE_ONLY + tag(SPLIT) + AUDIO)

# ONE tag - languages in two frames, then padding - with an ID3v1 tail at
# the end of the file: still one ID3v2 tag.
V1 = (b"TAG" + b"Title".ljust(30, b"\0") + b"Artist".ljust(30, b"\0") + b"Album".ljust(30, b"\0")
      + b"2020" + b"\0" * 30 + b"\xff")
open("one-tag-split-v1.mp3", "wb").write(tag(SPLIT, padding=256) + AUDIO + V1)


# ONE tag, then an APEv2 tag (header, one item, footer) and an ID3v1 tail
# at the end: still one ID3v2 tag.
def ape_item(key, value):
    return struct.pack("<II", len(value), 0) + key + b"\0" + value


ITEMS = ape_item(b"Title", b"Ape Title")
SIZE = len(ITEMS) + 32
APE_HEADER = b"APETAGEX" + struct.pack("<IIII", 2000, SIZE, 1, 0xA0000000) + b"\0" * 8
APE_FOOTER = b"APETAGEX" + struct.pack("<IIII", 2000, SIZE, 1, 0x80000000) + b"\0" * 8
open("one-tag-split-ape.mp3", "wb").write(tag(SPLIT) + AUDIO + APE_HEADER + ITEMS + APE_FOOTER + V1)

# ONE tag holding one language frame and 512 bytes of padding.
open("one-tag-padded.mp3", "wb").write(tag([frame(b"TLAN", b"eng")], padding=512) + AUDIO)

# A WAV file with two ID3 chunks, `id3 ` (no language) then `ID3 ` (`eng`).
run("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=8000:duration=0.1",
    "-c:a", "pcm_s16le", "-y", "tone.wav")
wav = bytearray(open("tone.wav", "rb").read())
os.remove("tone.wav")


def chunk(name, data):
    padded = data + (b"\0" if len(data) % 2 else b"")
    return name + struct.pack("<I", len(data)) + padded


wav += chunk(b"id3 ", TITLE_ONLY) + chunk(b"ID3 ", ONE_LANGUAGE)
wav[4:8] = struct.pack("<I", len(wav) - 8)
open("two-chunks-lang-in-second.wav", "wb").write(bytes(wav))
