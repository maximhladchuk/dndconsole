import { Panel } from '../../ui/Panel';
import { Slider } from '../../ui/Slider';
import { Toggle } from '../../ui/Toggle';
import type { AppSettings, Language } from '../../types/api';

interface SettingsPanelProps {
  settings: AppSettings;
  saving: boolean;
  onChange: (patch: Partial<AppSettings>) => void;
}

const LANGUAGES: { value: Language; label: string }[] = [
  { value: 'auto', label: 'Auto detect' },
  { value: 'uk', label: 'Українська' },
  { value: 'en', label: 'English' },
];

const ms = (v: number) => `${Math.round(v)} ms`;

export function SettingsPanel({ settings, saving, onChange }: SettingsPanelProps) {
  return (
    <>
      <Panel title="Audio" subtitle={saving ? 'Saving…' : 'Saved automatically'}>
        <Slider
          label="Master volume"
          value={settings.master_volume}
          onChange={(v) => onChange({ master_volume: v })}
        />
        <Slider
          label="Effects volume"
          value={settings.sfx_volume}
          onChange={(v) => onChange({ sfx_volume: v })}
        />
        <Slider
          label="Ambience volume"
          value={settings.ambience_volume}
          onChange={(v) => onChange({ ambience_volume: v })}
        />
        <Toggle
          label="Mute effects"
          checked={settings.effects_muted}
          onChange={(v) => onChange({ effects_muted: v })}
        />
        <Toggle
          label="Ignore the microphone while a sound is playing"
          hint="Stops sounds from being heard by the microphone and triggering more sounds. Anything said while a sound plays is not recognised, so turn this off if you use headphones."
          checked={settings.suppress_mic_during_playback}
          onChange={(v) => onChange({ suppress_mic_during_playback: v })}
        />
      </Panel>

      <Panel title="Speech" subtitle="Recognition runs entirely on this machine">
        <label className="field">
          <span className="field__label">Language</span>
          <select
            value={settings.language}
            onChange={(e) => onChange({ language: e.target.value as Language })}
          >
            {LANGUAGES.map((l) => (
              <option key={l.value} value={l.value}>
                {l.label}
              </option>
            ))}
          </select>
          <span className="field__hint">
            Picking a language rather than leaving this on Automatic makes recognition
            about four times faster: detecting the language costs a full encoder pass over
            every sentence. Automatic is only worth it if you switch languages mid-session.
          </span>
        </label>

        <div className="field">
          <span className="field__label">Speech model</span>
          <code className="field__mono">{settings.speech_model}</code>
          <span className="field__hint">Model download and switching arrive in Phase 5.</span>
        </div>
      </Panel>

      <Panel title="Detection" subtitle="Higher sensitivity triggers more often, and misfires more">
        <Slider
          label="Event sensitivity"
          value={settings.event_sensitivity}
          onChange={(v) => onChange({ event_sensitivity: v })}
        />
        <Slider
          label="Voice detection threshold"
          value={settings.vad_speech_threshold}
          onChange={(v) => onChange({ vad_speech_threshold: v })}
        />
        <Slider
          label="Silence before a phrase ends"
          value={settings.vad_silence_timeout_ms}
          min={200}
          max={2000}
          step={50}
          format={ms}
          onChange={(v) => onChange({ vad_silence_timeout_ms: v })}
        />
        <Slider
          label="Minimum speech length"
          value={settings.vad_min_speech_ms}
          min={50}
          max={1000}
          step={10}
          format={ms}
          onChange={(v) => onChange({ vad_min_speech_ms: v })}
        />
        <Slider
          label="Cut a monologue after"
          value={settings.vad_max_segment_ms}
          min={3000}
          max={30000}
          step={1000}
          format={(v) => `${Math.round(v / 1000)} s`}
          onChange={(v) => onChange({ vad_max_segment_ms: v })}
        />
        <span className="field__hint">
          Narration that never pauses is cut here so sounds still fire while you are
          talking. The cut is seamless — the next piece continues from the same audio, so
          nothing is lost mid-sentence.
        </span>
      </Panel>

      <Panel title="Application">
        <Toggle
          label="Debug mode"
          hint="Shows detection candidates, rejection reasons and stage timings."
          checked={settings.debug_mode}
          onChange={(v) => onChange({ debug_mode: v })}
        />
        <Toggle
          label="Managed sound library"
          hint="Copy imported sounds into the app directory instead of referencing them in place."
          checked={settings.managed_library}
          onChange={(v) => onChange({ managed_library: v })}
        />
      </Panel>
    </>
  );
}
