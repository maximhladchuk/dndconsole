import { useEffect, useId, useRef, useState } from 'react';

/**
 * A "?" that opens a short explanation next to whatever it labels.
 *
 * The alternative was a paragraph of hint text under every field. Six of those turn the
 * event editor into an essay you scroll past to reach the controls, and the explanation
 * that matters — what the numbers mean, and why a keyword alone does nothing — is longer
 * than a sentence, so it has nowhere to live.
 *
 * Click, not hover: hover tooltips cannot be read on a touchpad without the pointer
 * drifting off them, and this text is meant to be read, not glanced at.
 */
interface HintProps {
  /** Named so screen readers say what the button explains, not just "help". */
  label: string;
  children: React.ReactNode;
}

export function Hint({ label, children }: HintProps) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const root = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };

    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  return (
    <span className="hint" ref={root}>
      <button
        type="button"
        className="hint__button"
        aria-label={`Пояснення: ${label}`}
        aria-expanded={open}
        aria-controls={id}
        onClick={(event) => {
          // These buttons sit inside <label> elements. The spec excludes interactive
          // content from label activation, but the click still bubbles to handlers on
          // the way out, so it is stopped here rather than trusted.
          event.preventDefault();
          event.stopPropagation();
          setOpen((was) => !was);
        }}
      >
        ?
      </button>
      <div className="hint__popover" id={id} role="note" hidden={!open}>
        {children}
      </div>
    </span>
  );
}
