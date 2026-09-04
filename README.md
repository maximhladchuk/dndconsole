# dndsound

A voice-triggered sound engine for Dungeon Masters. It listens to your narration, works out
what just happened in the fiction, and plays the right sound — without you touching
anything.

> "You slowly push open the old wooden door."  →  `OPEN_DOOR`  →  `door_creak_07.wav`

## The rule this project is built around

**The finished application costs zero AI tokens to run and works with the network off.**

No OpenAI, no Anthropic, no Gemini, no cloud speech, no usage-based API of any kind at
runtime. Speech recognition, voice activity detection and semantic matching all run on the
user's own machine, from models downloaded once and then owned. Microphone audio never
leaves the computer. No telemetry.

AI helped build this. Using it never will.

## How it works

```
Microphone → VAD → local speech-to-text → normalization → event detection
          → trigger decision (threshold · cooldown · dedup) → sound selection → playback
```

Recognition does not map phrases to files. It maps narration to *game events*, and events
to *sound groups*:

```
"the door slowly swings open"  ┐
"he opens the wooden door"     ├─▶  OPEN_DOOR  ─▶  Wooden Doors  ─▶  door_creak_07.wav
"відчиняє двері"               ┘
```

Detection is deliberately biased toward silence: a missed sound is a small loss, a wrong
sound ruins the scene. "A sword lies on the table" plays nothing. "The knight swings his
sword" does.

## Stack

Tauri 2 · React 19 · TypeScript · Rust · SQLite ·
[whisper.cpp](https://github.com/ggml-org/whisper.cpp) (Metal) ·
[Silero VAD](https://github.com/snakers4/silero-vad) ·
[multilingual-e5-small](https://huggingface.co/intfloat/multilingual-e5-small) ·
[kira](https://docs.rs/kira)

Languages: Ukrainian and English, including mixed speech. The interface is in Ukrainian.

## Status

Phases 0–9 complete. The full path works end to end: microphone → voice activity
detection → local speech recognition → layered event detection → cooldowns and duplicate
suppression → sound playback, with Debug Mode, Text Simulation Mode and recorded-audio
mode for tuning.

Verified by 316 automated tests, including ones that play real audio, capture from the
real microphone, and transcribe real speech in Ukrainian and English. Detection holds
**100% recall and zero false positives** on a corpus of 240 narration lines and 28
lines that must stay silent.

Remaining: long-session soak testing, global hotkeys, and tuning against real Dungeon
Master recordings rather than synthetic speech.

## Sounds

The application ships a **manifest** of 218 public-domain sounds, not the audio. On first
launch it fetches them once — about 13 MB — into a cache it manages itself. After that
everything runs with the internet off.

There is no sound library to curate. Sounds arrive in thirty-six themed groups, an event
plays from a group, and the group picks one of its sounds while avoiding the last one it
played. What a user edits is the *phrasing* that fires an event, not the files.

The groups are deliberately narrow. "Magic" on its own was not useful — a fireball needs
an explosion, not a shimmer — so it is four groups: fireballs, healing, teleports, ice.

Every bundled sound is **CC0**, verified two ways: the manifest generator refuses to
resolve anything else, and a test re-checks the committed manifest. CC0 waives
attribution, so the application owes no credit lines — the author is recorded anyway,
because knowing where a file came from is useful even when nothing obliges it.

Fetching a sound at trigger time was measured and rejected: 230–300 ms per sound on a
good connection, on top of the ~800 ms the speech pipeline already costs, and silence at
the table whenever the network hiccups.

Importing local files still works in the backend and is covered by tests, but has no
interface. It was removed from the UI deliberately, so there is one way sounds arrive
rather than two that can disagree.

## Built-in events, and editing them

Thirty-six events ship with the application, defined in code
(`crates/detect/src/seed.rs`):

| Category | Events |
|---|---|
| Environment | doors opening, doors slamming, chests, bells, tavern |
| Combat | sword swings, drawing a blade, blocks and parries, armour, bows, a body hitting the ground, screams |
| Magic | spellcasting, fireballs, healing, teleports, ice |
| Creatures | wolves, dragons, horses, ghosts, bones and skeletons, crows, small creatures |
| Weather | thunder, wind, rain |
| Items | coins, potions, dice, keys and locks, scrolls and maps |
| Other | fire, water, footsteps, breaking glass |

They are re-applied from code on every startup, so a fix to the phrasing reaches
installations that already exist. **The moment you edit an event, that stops** — your
version is kept and the built-in one never overwrites it again. The editor says which
state an event is in and offers to reset it.

That mechanism exists because of a real failure: an ordinary Ukrainian verb was missing
from the sword event, and adding it changed nothing for anyone who had already launched
the application, because the seed only ever ran on an empty database.

## Installing

Download a build from [Releases](https://github.com/maximhladchuk/dndconsole/releases) —
`.dmg` for macOS, `.exe`/`.msi` for Windows. Neither is code-signed, so the first launch
needs one click past Gatekeeper or SmartScreen; **[INSTALL.md](INSTALL.md)** (Ukrainian)
has the exact steps, the measured system requirements, and how to build from source.

Verified on macOS (Apple Silicon). Windows is **not verified**: the dependency graph
resolves for `x86_64-pc-windows-msvc`, the pure-Rust crates cross-compile, and the one
blocker that existed — whisper.cpp's `metal` feature, which does not exist off Apple
platforms — is now selected per target. Nobody has built or run it on Windows.

Without Metal, whisper runs on the CPU: measured on the same machine with the backend
switched off, a sentence decodes in 2.3 s instead of 0.9 s, and triggering *mid*-sentence
stops keeping up.
