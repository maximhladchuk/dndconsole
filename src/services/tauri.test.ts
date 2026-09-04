import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { BackendError, call } = await import('./tauri');

describe('call', () => {
  // Block body, not a concise arrow: Vitest treats a value returned from a hook as a
  // teardown callback, so returning the mock would call it after each test.
  beforeEach(() => {
    invoke.mockReset();
  });

  it('passes the command and arguments through and returns the result', async () => {
    invoke.mockResolvedValue({ ok: true });
    await expect(call('app_status', { id: 1 })).resolves.toEqual({ ok: true });
    expect(invoke).toHaveBeenCalledWith('app_status', { id: 1 });
  });

  it('turns a structured backend rejection into a BackendError with its kind', async () => {
    invoke.mockRejectedValue({ kind: 'notFound', message: 'profile 7 not found' });

    await expect(call('get')).rejects.toMatchObject({
      name: 'BackendError',
      kind: 'notFound',
      message: 'profile 7 not found',
    });
  });

  it('never swallows an unstructured rejection', async () => {
    invoke.mockRejectedValue('everything broke');

    const error = await call('get').catch((e: unknown) => e);
    expect(error).toBeInstanceOf(BackendError);
    expect((error as InstanceType<typeof BackendError>).kind).toBe('unknown');
    expect((error as Error).message).toContain('everything broke');
  });
});
