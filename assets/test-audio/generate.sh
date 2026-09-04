#!/usr/bin/env bash
# Generate speech fixtures for the automated tests, using macOS text-to-speech.
#
# These are synthetic voices, not real narration. They are deterministic and free,
# which makes them good for regression-testing the pipeline's plumbing: voice activity
# detection, transcription, and the path from speech to a played sound. Tuning
# *detection quality* still needs real Dungeon Master recordings.
#
# Output: 16 kHz mono 16-bit WAV, the format the pipeline works in.
set -euo pipefail
cd "$(dirname "$0")"

# Speech, padded with silence at both ends so the segmenter has an edge to find.
emit() {
  local name="$1" voice="$2" text="$3"
  say -v "$voice" -o "/tmp/dndsound-$name.wav" \
      --file-format=WAVE --data-format=LEI16@16000 "$text"
  python3 - "$name" <<'PY'
import sys, wave
name = sys.argv[1]
with wave.open(f"/tmp/dndsound-{name}.wav", "rb") as src:
    frames = src.readframes(src.getnframes())
pad = b"\x00\x00" * int(16000 * 0.4)
with wave.open(f"{name}.wav", "wb") as dst:
    dst.setnchannels(1); dst.setsampwidth(2); dst.setframerate(16000)
    dst.writeframes(pad + frames + pad)
PY
  rm -f "/tmp/dndsound-$name.wav"
  echo "  $name.wav"
}

echo "generating speech fixtures:"
emit en_open_door  Samantha "You slowly push open the old wooden door."
emit uk_open_door  Lesya    "Ви повільно відчиняєте старі дерев'яні двері."
emit en_sword      Samantha "The goblin pulls out his sword and swings at you."
emit uk_sword      Lesya    "Гоблін дістає меч і різко б'є по тобі."
emit en_no_action  Samantha "You see a sword lying on the table."
emit mixed_uk_en   Lesya    "Гоблін дістає sword і атакує тебе."

# Two sentences separated by a long gap, for segmentation tests.
say -v Samantha -o /tmp/dndsound-a.wav --file-format=WAVE --data-format=LEI16@16000 \
    "You open the door."
say -v Samantha -o /tmp/dndsound-b.wav --file-format=WAVE --data-format=LEI16@16000 \
    "Thunder rolls overhead."
python3 - <<'PY'
import wave
def frames(path):
    with wave.open(path, "rb") as f:
        return f.readframes(f.getnframes())
gap = b"\x00\x00" * int(16000 * 1.5)
with wave.open("two_sentences.wav", "wb") as dst:
    dst.setnchannels(1); dst.setsampwidth(2); dst.setframerate(16000)
    dst.writeframes(gap + frames("/tmp/dndsound-a.wav") + gap
                    + frames("/tmp/dndsound-b.wav") + gap)
print("  two_sentences.wav")
PY
rm -f /tmp/dndsound-a.wav /tmp/dndsound-b.wav

python3 - <<'PY'
import wave
with wave.open("silence.wav", "wb") as dst:
    dst.setnchannels(1); dst.setsampwidth(2); dst.setframerate(16000)
    dst.writeframes(b"\x00\x00" * 16000 * 2)
print("  silence.wav")
PY
