# Installing dndsound

There is no signed installer yet, so the application is built from source. On a machine
that already has the tools this takes about ten minutes, most of which is Rust compiling
whisper.cpp the first time.

Everything below was done on **macOS 26.6 (Apple Silicon)**. Linux and Windows are not
tested; the notes at the end say what would have to change.

---

## 1. Install the tools

| Tool | Version | Why |
|---|---|---|
| Xcode Command Line Tools | any current | C/C++ toolchain for whisper.cpp |
| Rust | 1.98 or newer | the backend |
| CMake | 3.20 or newer | builds whisper.cpp |
| Node.js | **22.12+** (22.21.1 is pinned) | the interface |

```sh
xcode-select --install

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"

brew install cmake
brew install nvm    # or install Node 22 however you prefer
```

Node 20.11 is **too old** — Vite 7 fails on it with `crypto.hash is not a function`. The
repository pins the version in `.nvmrc`, so `nvm use` picks the right one.

The project uses **npm**. `pnpm` via corepack fails on this machine with
`ERR_VM_DYNAMIC_IMPORT_CALLBACK_MISSING`.

## 2. Get the code and its dependencies

```sh
git clone https://github.com/maximhladchuk/dndconsole.git
cd dndconsole

nvm use          # Node 22.21.1, from .nvmrc
npm install
```

## 3. Run it

```sh
npm run tauri dev
```

The first build compiles whisper.cpp and the whole Rust workspace — expect several
minutes and a lot of output. Later runs start in seconds.

To build a distributable application instead:

```sh
npm run tauri build
```

The result lands in `src-tauri/target/release/bundle/` — a `.app` and a `.dmg`.

> An unsigned build will be refused by Gatekeeper on another Mac. Until the app is
> signed and notarised, right-click the `.app` → **Open** on first launch, or run
> `xattr -dr com.apple.quarantine /path/to/dndsound.app`.

## 4. First launch — the Setup tab

The application opens on **Setup**, which is an ordered checklist. Work down it:

1. **Voice detection model** — Silero VAD, about 2 MB. Required. It is what decides when
   someone is speaking, so nothing works without it.
2. **Speech model** — Whisper `large-v3-turbo-q5_0`, about 570 MB. Required. There is no
   choice of model on purpose: this is the best one that keeps up in real time on Apple
   Silicon.
3. **Sounds** — about 220 public-domain sounds fetched once from Freesound. After this the
   application never needs the network again.
4. **Microphone** — macOS asks for permission the first time a session starts. If you
   miss the prompt, grant it in **System Settings › Privacy & Security › Microphone**.

Steps 1–3 download over the network and can be re-run safely; a file that is already
present and verified is not fetched again.

The optional step at the bottom (a small embedding model plus a faster Whisper) improves
matching and lets sounds fire mid-sentence. Everything works without it.

## 5. Use it

* **Session** — press *Start listening* and narrate. Transcripts, detections and played
  sounds appear as they happen.
* **Events** — the phrases each event listens for. They ship in code and you can change
  them; the moment you edit one, it stops being overwritten by updates.
* **Sounds** — the themed groups and what is in each. Read-only: sounds arrive from the
  pack, not from your disk.
* **Settings** — volumes, language, sensitivity, and **"Ignore the microphone while a
  sound is playing"**. Leave that on if you use speakers — otherwise a thunderclap the
  app plays is heard by the microphone and can trigger another sound. Turn it off on
  headphones, where there is no loop and speech during a sound would be lost for nothing.

### Pick a language

Settings › Speech › Language. Choosing **Українська** or **English** instead of
*Auto detect* makes recognition roughly **four times faster**: detecting the language
costs a full encoder pass over every sentence. Auto is only worth it if you switch
languages mid-session.

---

## Where things are kept

```
~/Library/Application Support/com.maximhladchuk.dndsound/
  dndsound.db      settings, events, sound groups
  models/          downloaded models
  library/         the downloaded sound pack
```

Deleting that directory resets the application to a fresh install.

## Running the tests

```sh
. "$HOME/.cargo/env"

cargo test                                  # Rust workspace
cargo clippy --all-targets --all-features
cargo fmt --check

npm test          # frontend
npm run typecheck
npm run lint
```

Some tests open the real microphone, play real audio and transcribe real speech; they
skip themselves when the hardware or the models are absent rather than failing.

## Privacy

Microphone audio never leaves the machine, and there is no telemetry. The only network
traffic is downloading models and the sound pack, both of which happen once and are
visible as steps you press.

## Other platforms

Linux and Windows are not tested. What would need attention:

* whisper.cpp is built with **Metal** here; on other platforms it falls back to CPU or
  needs a different acceleration backend.
* `cpal` picks a different host API (ALSA/PulseAudio, WASAPI), so device naming and
  permissions differ.
* Tauri needs its own [platform prerequisites](https://v2.tauri.app/start/prerequisites/).

## Troubleshooting

| Symptom | Cause |
|---|---|
| `crypto.hash is not a function` during `npm install` or dev | Node is too old. `nvm use`. |
| `cargo: command not found` in a new shell | rustup was installed without touching PATH. Run `. "$HOME/.cargo/env"` or add `~/.cargo/bin` to your profile. |
| Build fails in `whisper-rs-sys` | CMake or the Xcode Command Line Tools are missing. |
| *Start listening* says the model is missing | Step 1 or 2 of Setup has not finished. |
| Nothing triggers, but transcripts appear | Look in **Events** — the phrase you used may not be listed. Turn on Debug Mode in Settings to see every rejected candidate and why. |
| The app plays a sound and then immediately plays another | Turn on "Ignore the microphone while a sound is playing" in Settings. |
