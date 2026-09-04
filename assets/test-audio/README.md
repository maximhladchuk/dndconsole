# Speech fixtures for automated tests

Generated with macOS text-to-speech (`say`), 16 kHz mono 16-bit WAV — the format the
pipeline works in. Regenerate with:

```
./assets/test-audio/generate.sh
```

**What these are good for:** regression-testing the plumbing. Voice activity detection
finds the speech, Whisper transcribes it, the detector maps the transcript to an event,
the right sound plays. All of that is deterministic and can be asserted.

**What they are not:** real narration. Synthetic voices are cleaner, flatter and more
predictable than a Dungeon Master at a noisy table. Detection thresholds tuned only
against these would be tuned against the wrong thing. Real recordings are the next step,
via the recorded-audio test mode.
