interface ErrorBannerProps {
  kind: string;
  message: string;
  tone?: 'error' | 'info';
  onDismiss: () => void;
}

/**
 * Errors are always shown, never logged and forgotten. `kind` is displayed too, since
 * it is the stable identifier worth quoting in a bug report.
 */
export function ErrorBanner({ kind, message, tone = 'error', onDismiss }: ErrorBannerProps) {
  return (
    <div className={tone === 'info' ? 'error-banner is-info' : 'error-banner'} role="alert">
      <div>
        <strong>{kind}</strong>
        <p>{message}</p>
      </div>
      <button type="button" onClick={onDismiss} aria-label="Dismiss">
        ×
      </button>
    </div>
  );
}
