import { useEffect } from 'react';

import { useAppStore } from '../../stores/appStore';
import { useCaptureStore } from '../../stores/captureStore';
import { Panel } from '../../ui/Panel';
import { LevelMeter } from '../../ui/LevelMeter';

/**
 * Microphone selection and the live input level.
 *
 * "Listening" here means audio is being captured and resampled to 16 kHz, without the
 * recognition pipeline attached — it is how the capture path is proved on its own.
 */
export function MicrophonePanel() {
  const devices = useCaptureStore((s) => s.devices);
  const snapshot = useCaptureStore((s) => s.snapshot);
  const loadDevices = useCaptureStore((s) => s.loadDevices);
  const start = useCaptureStore((s) => s.start);
  const stop = useCaptureStore((s) => s.stop);

  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  useEffect(() => {
    void loadDevices();
  }, [loadDevices]);

  const listening = snapshot?.listening ?? false;
  const selected = settings?.input_device ?? '';

  return (
    <Panel
      title="Мікрофон"
      subtitle={listening ? 'Слухаю — звук не залишає цей комп’ютер' : 'Не слухаю'}
    >
      <label className="field">
        <span className="field__label">Пристрій входу</span>
        <select
          value={selected}
          disabled={listening}
          onChange={(e) => void updateSettings({ input_device: e.target.value || null })}
        >
          <option value="">Системний типовий</option>
          {devices.map((device) => (
            <option key={device.name} value={device.name}>
              {device.name}
              {device.isDefault ? ' (типовий)' : ''}
            </option>
          ))}
        </select>
        <span className="field__hint">
          {listening
            ? 'Щоб змінити пристрій, зупини прослуховування.'
            : `Знайдено пристроїв: ${devices.length}.`}
        </span>
      </label>

      <LevelMeter level={snapshot?.level ?? 0} active={listening} />

      <div className="library__actions">
        {listening ? (
          <button type="button" onClick={() => void stop()}>
            Зупинити
          </button>
        ) : (
          <button type="button" onClick={() => void start()}>
            Слухати
          </button>
        )}
        <button type="button" onClick={() => void loadDevices()} disabled={listening}>
          Оновити список
        </button>
      </div>

      {snapshot?.deviceName ? (
        <p className="field__hint">
          {snapshot.deviceName}
          {snapshot.inputSampleRate ? ` · ${snapshot.inputSampleRate} Гц` : ''}
          {snapshot.inputChannels ? ` · ${snapshot.inputChannels} кан.` : ''}
          {' → 16 кГц моно'}
        </p>
      ) : null}
    </Panel>
  );
}
