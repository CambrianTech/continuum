import { describe, expect, it } from 'vitest';

import { CORE_SOCKET_ENV, coreEndpoint } from './coreEndpoint';

describe('core endpoint', () => {
  // what this catches (card 4e8b1a92): a missing endpoint silently becoming a guessed
  // /tmp path, which is wrong on Windows; and a blank value read as an endpoint.
  it('uses the endpoint the launcher passed, and names the variable when there is none', () => {
    expect(coreEndpoint({ [CORE_SOCKET_ENV]: 'C:\\Temp\\continuum-core.sock' })).toEqual({
      ok: true,
      endpoint: 'C:\\Temp\\continuum-core.sock',
    });
    expect(coreEndpoint({ [CORE_SOCKET_ENV]: 'tcp://127.0.0.1:7777' })).toEqual({
      ok: true,
      endpoint: 'tcp://127.0.0.1:7777',
    });
    for (const env of [{}, { [CORE_SOCKET_ENV]: '   ' }]) {
      const missing = coreEndpoint(env);
      expect(missing.ok).toBe(false);
      if (!missing.ok) {
        expect(missing.reason).toContain(CORE_SOCKET_ENV);
        expect(missing.reason).not.toContain('/tmp');
      }
    }
  });
});
