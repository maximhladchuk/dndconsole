import { useEffect, useState } from 'react';

import { modelsService } from '../../services/models';
import { microphoneService } from '../../services/microphone';
import { useSoundPackStore } from '../../stores/soundPackStore';
import { Panel } from '../../ui/Panel';
import { formatBytes } from '../../domain/audio';
import type { DownloadUpdate, ModelInfo } from '../../types/api';

/**
 * Everything that has to happen once before the application can hear anything.
 *
 * Presented as an ordered checklist rather than a catalogue. There is nothing to choose
 * between: the voice-activity model is required, the speech model is the best one
 * available, and picking between quantisations is not a decision to put in front of a
 * Dungeon Master who wants to start a game.
 */

/** Improves matching but the other three detection layers work without it. */
const OPTIONAL_MODELS = [
  'multilingual-e5-small-int8',
  'multilingual-e5-small-tokenizer',
  'small-q5_1',
] as const;

interface StepProps {
  index: number;
  title: string;
  detail: string;
  done: boolean;
  optional?: boolean;
  /** What is happening right now, shown in place of `detail`. */
  busy?: string | null;
  /** 0–100 while this step is working, so the wait has a visible shape. */
  percent?: number | null;
  /** Something else on this screen is working; this step's button waits its turn. */
  blocked?: boolean;
  action?: { label: string; run: () => void };
}

function Step({
  index,
  title,
  detail,
  done,
  optional,
  busy,
  percent,
  blocked,
  action,
}: StepProps) {
  const working = busy !== null && busy !== undefined;

  return (
    <li className={done ? 'step is-done' : working ? 'step is-working' : 'step'}>
      <span className="step__mark" aria-hidden="true">
        {working ? <span className="spinner" /> : done ? '✓' : index}
      </span>
      <span className="step__body">
        <strong>
          {title}
          {optional ? <span className="step__optional"> · необов’язково</span> : null}
        </strong>
        <span className="step__detail">{busy ?? detail}</span>
        {working ? (
          <span
            className="level"
            role="progressbar"
            aria-valuenow={percent ?? undefined}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <span
              className={percent === null ? 'level__fill is-indeterminate' : 'level__fill'}
              style={percent === null ? undefined : { width: `${percent ?? 0}%` }}
            />
          </span>
        ) : null}
      </span>
      {action && !done ? (
        <button type="button" onClick={action.run} disabled={working || blocked}>
          {action.label}
        </button>
      ) : null}
    </li>
  );
}

export function SetupPage() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadUpdate>>({});
  const [micReady, setMicReady] = useState<boolean | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // Shared with the sound pack panel on the library screen, so an install started in
  // one place shows its progress in the other.
  const pack = useSoundPackStore();
  const packReady = pack.status === null ? null : pack.status.installed;

  const refresh = async () => {
    try {
      const [list, devices] = await Promise.all([modelsService.list(), microphoneService.list()]);
      setModels(list);
      setMicReady(devices.length > 0);
      await pack.refresh();
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    }
  };

  useEffect(() => {
    void refresh();

    const subscription = modelsService.subscribe((update) =>
      setProgress((current) => ({ ...current, [update.id]: update })),
    );
    return () => {
      void subscription.then((unsubscribe) => unsubscribe());
    };
    // Once, on mount. `refresh` closes over the pack store and is rebuilt every render;
    // listing it here would re-subscribe on each one.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const has = (id: string) => models.find((m) => m.id === id)?.downloaded ?? false;
  const missing = (ids: readonly string[]) => ids.filter((id) => !has(id));

  const sizeOf = (ids: readonly string[]) =>
    ids.reduce((total, id) => total + (models.find((m) => m.id === id)?.approxBytes ?? 0), 0);

  /** The in-flight download for a step, if one of its models is being fetched. */
  const active = (ids: readonly string[]) => ids.map((id) => progress[id]).find((p) => p && !p.done);

  const downloading = (ids: readonly string[]) => {
    const update = active(ids);
    if (!update) return null;
    const percent = percentOf(ids);
    return percent === null ? 'Завантаження…' : `Завантаження… ${percent}%`;
  };

  const percentOf = (ids: readonly string[]) => {
    const update = active(ids);
    if (!update?.totalBytes) return null;
    return Math.round((update.downloadedBytes / update.totalBytes) * 100);
  };

  const fetchAll = async (ids: readonly string[]) => {
    setBusy('working');
    setNotice(null);
    try {
      for (const id of missing(ids)) {
        await modelsService.download(id);
      }
      await refresh();
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  const installPack = async () => {
    setNotice(null);
    // The store owns `installing` and the per-sound progress; this only reacts to it.
    await pack.install();
    await refresh();
  };

  const packPercent = pack.progress
    ? Math.round((pack.progress.done / Math.max(pack.progress.total, 1)) * 100)
    : null;

  const vadDone = has('silero-vad-16k');
  const speechDone = has('large-v3-turbo-q5_0');
  const optionalDone = OPTIONAL_MODELS.every(has);
  const ready = vadDone && speechDone && packReady === true && micReady === true;

  return (
    <div className="app__grid">
      <div className="app__column">
        <Panel
          title="Налаштування"
          subtitle={
            ready
              ? 'Усе готово — відкривай «Сесія» і розповідай'
              : 'Чотири кроки, які робляться один раз. Далі все працює без інтернету.'
          }
        >
          {notice ? <p className="empty">{notice}</p> : null}
          {pack.error ? <p className="empty">{pack.error.message}</p> : null}

          <ol className="steps">
            <Step
              index={1}
              title="Чути, коли ти говориш"
              detail={`Крихітна модель визначення мови, ${formatBytes(sizeOf(['silero-vad-16k']))}. Без неї нічого не працює.`}
              done={vadDone}
              busy={downloading(['silero-vad-16k'])}
              percent={percentOf(['silero-vad-16k'])}
              blocked={busy !== null || pack.installing}
              action={{ label: 'Завантажити', run: () => void fetchAll(['silero-vad-16k']) }}
            />
            <Step
              index={2}
              title="Розуміти слова"
              detail={`Розпізнавання мови, ${formatBytes(sizeOf(['large-v3-turbo-q5_0']))}. Найкраще з доступного для української та англійської.`}
              done={speechDone}
              busy={downloading(['large-v3-turbo-q5_0'])}
              percent={percentOf(['large-v3-turbo-q5_0'])}
              blocked={busy !== null || pack.installing}
              action={{ label: 'Завантажити', run: () => void fetchAll(['large-v3-turbo-q5_0']) }}
            />
            <Step
              index={3}
              title="Самі звуки"
              detail={
                pack.status
                  ? `Набір звуків із суспільного надбання, ${pack.status.total} файлів, близько ${pack.status.megabytes.toFixed(0)} МБ.`
                  : 'Набір звуків із суспільного надбання, близько 13 МБ.'
              }
              done={packReady === true}
              busy={
                pack.installing
                  ? pack.progress
                    ? `Завантажую ${pack.progress.done} з ${pack.progress.total} — ${pack.progress.current}`
                    : 'Починаю…'
                  : null
              }
              percent={packPercent}
              blocked={busy !== null}
              action={{ label: 'Завантажити', run: () => void installPack() }}
            />
            <Step
              index={4}
              title="Мікрофон"
              detail={
                micReady === true
                  ? 'Знайдено. Обрати конкретний можна на вкладці «Сесія».'
                  : 'Пристрій входу не знайдено. Дозволь доступ до мікрофона в налаштуваннях системи.'
              }
              done={micReady === true}
              blocked={busy !== null || pack.installing}
              action={{ label: 'Перевірити ще раз', run: () => void refresh() }}
            />
            <Step
              index={5}
              title="Краще розпізнавання"
              detail={`Розуміє оповідь, у якій немає жодного зі вказаних слів, і зіставляє українську мову з англійськими фразами. ${formatBytes(sizeOf(OPTIONAL_MODELS))}.`}
              done={optionalDone}
              optional
              busy={downloading(OPTIONAL_MODELS)}
              percent={percentOf(OPTIONAL_MODELS)}
              blocked={busy !== null || pack.installing}
              action={{ label: 'Завантажити', run: () => void fetchAll(OPTIONAL_MODELS) }}
            />
          </ol>
        </Panel>
      </div>

      <div className="app__column">
        <Panel title="Сховище" subtitle="Що завантажено і скільки займає">
          <ul className="sound-list">
            {models.map((model) => (
              <li key={model.id} className={model.downloaded ? 'sound-row' : 'sound-row is-muted'}>
                <span className="sound-row__name">
                  {model.displayName}
                  <span className="sound-row__meta">
                    {formatBytes(model.approxBytes)} · {model.license} ·{' '}
                    {model.downloaded ? 'встановлено' : 'не завантажено'}
                  </span>
                </span>
                {model.downloaded ? (
                  <span className="sound-row__actions">
                    <button
                      type="button"
                      onClick={() => {
                        void modelsService.remove(model.id).then(refresh);
                      }}
                    >
                      Видалити
                    </button>
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        </Panel>
      </div>
    </div>
  );
}
