import { useEffect } from 'react';

import { useAppStore } from '../stores/appStore';

/** Loads status, settings and profiles once, when the app mounts. */
export function useBootstrap(): void {
  const load = useAppStore((s) => s.load);
  useEffect(() => {
    void load();
  }, [load]);
}
