import type { ReactNode } from 'react';

interface PanelProps {
  title: string;
  subtitle?: string;
  children: ReactNode;
}

export function Panel({ title, subtitle, children }: PanelProps) {
  return (
    <section className="panel">
      <header className="panel__header">
        <h2>{title}</h2>
        {subtitle ? <p className="panel__subtitle">{subtitle}</p> : null}
      </header>
      <div className="panel__body">{children}</div>
    </section>
  );
}
