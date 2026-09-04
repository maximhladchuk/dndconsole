import { Hint } from '../../ui/Hint';
import { Panel } from '../../ui/Panel';
import { Slider } from '../../ui/Slider';
import { Toggle } from '../../ui/Toggle';
import type { AppSettings, Language } from '../../types/api';

import {
  DEBUG_HINT,
  LANGUAGE_HINT,
  MAX_SEGMENT_HINT,
  MIN_SPEECH_HINT,
  SENSITIVITY_HINT,
  SILENCE_HINT,
  SUPPRESS_MIC_HINT,
  VAD_THRESHOLD_HINT,
} from './hints';

interface SettingsPanelProps {
  settings: AppSettings;
  saving: boolean;
  onChange: (patch: Partial<AppSettings>) => void;
}

const LANGUAGES: { value: Language; label: string }[] = [
  { value: 'auto', label: 'Визначати автоматично' },
  { value: 'uk', label: 'Українська' },
  { value: 'en', label: 'English' },
];

const ms = (v: number) => `${Math.round(v)} мс`;

export function SettingsPanel({ settings, saving, onChange }: SettingsPanelProps) {
  return (
    <>
      <Panel title="Звук" subtitle={saving ? 'Зберігаю…' : 'Зберігається автоматично'}>
        <Slider
          label="Загальна гучність"
          value={settings.master_volume}
          onChange={(v) => onChange({ master_volume: v })}
        />
        <Slider
          label="Гучність ефектів"
          value={settings.sfx_volume}
          onChange={(v) => onChange({ sfx_volume: v })}
        />
        <Slider
          label="Гучність фону"
          value={settings.ambience_volume}
          onChange={(v) => onChange({ ambience_volume: v })}
        />
        <Toggle
          label="Вимкнути ефекти"
          checked={settings.effects_muted}
          onChange={(v) => onChange({ effects_muted: v })}
        />
        <Toggle
          label="Не слухати мікрофон, поки грає звук"
          explain={SUPPRESS_MIC_HINT}
          hint="Не дає програмі почути власний звук і запустити ще один."
          checked={settings.suppress_mic_during_playback}
          onChange={(v) => onChange({ suppress_mic_during_playback: v })}
        />
      </Panel>

      <Panel title="Мова" subtitle="Розпізнавання працює лише на цьому комп’ютері">
        <label className="field">
          <span className="field__label">
            <span className="field__label-text">
              Мова
              <Hint label="вибір мови">{LANGUAGE_HINT}</Hint>
            </span>
          </span>
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
            Обрана мова замість автоматичної — приблизно вчетверо швидше розпізнавання.
          </span>
        </label>

        <div className="field">
          <span className="field__label">Модель розпізнавання</span>
          <code className="field__mono">{settings.speech_model}</code>
          <span className="field__hint">
            Завантажується на вкладці «Налаштування». Вибору немає навмисно — ставиться
            найкраща доступна.
          </span>
        </div>
      </Panel>

      <Panel title="Розпізнавання подій" subtitle="Вища чутливість — частіші спрацювання і частіші помилки">
        <Slider
          label="Чутливість подій"
          explain={SENSITIVITY_HINT}
          value={settings.event_sensitivity}
          onChange={(v) => onChange({ event_sensitivity: v })}
        />
        <Slider
          label="Поріг виявлення голосу"
          explain={VAD_THRESHOLD_HINT}
          value={settings.vad_speech_threshold}
          onChange={(v) => onChange({ vad_speech_threshold: v })}
        />
        <Slider
          label="Тиша, після якої фраза вважається завершеною"
          explain={SILENCE_HINT}
          value={settings.vad_silence_timeout_ms}
          min={200}
          max={2000}
          step={50}
          format={ms}
          onChange={(v) => onChange({ vad_silence_timeout_ms: v })}
        />
        <Slider
          label="Найкоротша мова"
          explain={MIN_SPEECH_HINT}
          value={settings.vad_min_speech_ms}
          min={50}
          max={1000}
          step={10}
          format={ms}
          onChange={(v) => onChange({ vad_min_speech_ms: v })}
        />
        <Slider
          label="Різати монолог через"
          explain={MAX_SEGMENT_HINT}
          value={settings.vad_max_segment_ms}
          min={3000}
          max={30000}
          step={1000}
          format={(v) => `${Math.round(v / 1000)} с`}
          onChange={(v) => onChange({ vad_max_segment_ms: v })}
        />
        <span className="field__hint">
          Оповідь без пауз ріжеться тут, щоб звуки лунали, поки ти ще говориш.
        </span>
      </Panel>

      <Panel title="Програма">
        <Toggle
          label="Режим діагностики"
          explain={DEBUG_HINT}
          hint="Показує кандидатів, причини відмов і час кожного етапу."
          checked={settings.debug_mode}
          onChange={(v) => onChange({ debug_mode: v })}
        />
        <Toggle
          label="Копіювати звуки до себе"
          hint="Імпортовані файли копіюються в теку програми, а не використовуються на місці."
          checked={settings.managed_library}
          onChange={(v) => onChange({ managed_library: v })}
        />
      </Panel>
    </>
  );
}
