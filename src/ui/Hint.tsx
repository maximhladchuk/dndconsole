import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

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
 *
 * The popover is rendered into `document.body` rather than next to its button. An
 * absolutely positioned child still counts toward its scroll container's overflow, so
 * the first version widened the panel it sat in and gave it a horizontal scrollbar. A
 * portal has no parent to widen.
 */
interface HintProps {
  /** Named so screen readers say what the button explains, not just "help". */
  label: string;
  children: React.ReactNode;
}

/** Widest the popover gets, before the viewport is allowed to shrink it. */
const MAX_WIDTH = 420;
/** Kept clear of every viewport edge. */
const MARGIN = 12;
const GAP = 8;

interface Position {
  top: number;
  left: number;
  width: number;
}

export function Hint({ label, children }: HintProps) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<Position | null>(null);
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const popover = useRef<HTMLDivElement>(null);

  const place = useCallback(() => {
    const anchor = button.current?.getBoundingClientRect();
    if (!anchor) return;

    const width = Math.min(MAX_WIDTH, window.innerWidth - MARGIN * 2);
    const left = Math.min(
      Math.max(MARGIN, anchor.left - GAP),
      window.innerWidth - width - MARGIN,
    );

    // Below the button, unless that runs off the bottom — then above it.
    const height = popover.current?.offsetHeight ?? 0;
    const below = anchor.bottom + GAP;
    const top =
      height > 0 && below + height > window.innerHeight - MARGIN
        ? Math.max(MARGIN, anchor.top - GAP - height)
        : below;

    setPosition({ top, left, width });
  }, []);

  // Two passes on purpose: the first renders the popover so it has a height, the second
  // uses that height to decide whether it fits below the button. It is invisible until
  // placed, so the intermediate position is never seen.
  useLayoutEffect(() => {
    if (open) place();
    else setPosition(null);
  }, [open, place]);

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!button.current?.contains(target) && !popover.current?.contains(target)) {
        setOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };

    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    // Capture, because the scroll that moves the button happens on a panel, not on the
    // window, and a scroll event on a child does not bubble.
    window.addEventListener('scroll', place, true);
    window.addEventListener('resize', place);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('scroll', place, true);
      window.removeEventListener('resize', place);
    };
  }, [open, place]);

  return (
    <span className="hint">
      <button
        ref={button}
        type="button"
        className="hint__button"
        aria-label={`Пояснення: ${label}`}
        aria-expanded={open}
        aria-controls={id}
        onClick={(event) => {
          // These buttons sit inside <label> elements. The HTML spec excludes interactive
          // content from label activation, but the click still bubbles to handlers on
          // the way out, so it is stopped here rather than trusted.
          event.preventDefault();
          event.stopPropagation();
          setOpen((was) => !was);
        }}
      >
        ?
      </button>

      {open
        ? createPortal(
            <div
              ref={popover}
              className="hint__popover"
              id={id}
              role="note"
              style={{
                top: position?.top ?? 0,
                left: position?.left ?? 0,
                width: position?.width ?? MAX_WIDTH,
                visibility: position ? 'visible' : 'hidden',
              }}
            >
              {children}
            </div>,
            document.body,
          )
        : null}
    </span>
  );
}
