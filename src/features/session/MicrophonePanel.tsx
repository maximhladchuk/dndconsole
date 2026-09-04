import { useEffect } from 'react';

import { useAppStore } from '../../stores/appStore';
import { useCaptureStore } from '../../stores/captureStore';
import { Panel } from '../../ui/Panel';
import { LevelMeter } from '../../ui/LevelMeter';

/**
 * Microphone selection and the live input level.
 *
 * "Listening" here means audio is being captured and resampled to 16 kHz. Speech
 * recognition arrives in Phase 5; until then this proves the capture path end to end,
 * which is exactly what the meter is for.
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
      title="Microphone"
      subtitle={listening ? 'Listening — audio stays on this machine' : 'Not listening'}
    >
      <label className="field">
        <span className="field__label">Input device</span>
        <select
          value={selected}
          disabled={listening}
          onChange={(e) => void updateSettings({ input_device: e.target.value || null })}
        >
          <option value="">System default</option>
          {devices.map((device) => (
            <option key={device.name} value={device.name}>
              {device.name}
              {device.isDefault ? ' (default)' : ''}
            </option>
          ))}
        </select>
        <span className="field__hint">
          {listening
            ? 'Stop listening to change device.'
            : `${devices.length} input${devices.length === 1 ? '' : 's'} found.`}
        </span>
      </label>

      <LevelMeter level={snapshot?.level ?? 0} active={listening} />

      <div className="library__actions">
        {listening ? (
          <button type="button" onClick={() => void stop()}>
            Stop listening
          </button>
        ) : (
          <button type="button" onClick={() => void start()}>
            Start listening
          </button>
        )}
        <button type="button" onClick={() => void loadDevices()} disabled={listening}>
          Refresh devices
        </button>
      </div>

      {snapshot?.deviceName ? (
        <p className="field__hint">
          {snapshot.deviceName}
          {snapshot.inputSampleRate ? ` · ${snapshot.inputSampleRate} Hz` : ''}
          {snapshot.inputChannels ? ` · ${snapshot.inputChannels} ch` : ''}
          {' → 16 kHz mono'}
        </p>
      ) : null}
    </Panel>
  );
}
