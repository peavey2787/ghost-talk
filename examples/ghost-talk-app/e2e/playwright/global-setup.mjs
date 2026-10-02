// Every run (scripted or direct `npx playwright test`) starts from funded,
// consolidated instance wallets so repeated runs cannot drain them.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const exe = process.platform === 'win32' ? 'ghost-e2e.exe' : 'ghost-e2e';

export default function globalSetup() {
  const harness = path.join(here, '..', 'harness', 'target', 'release', exe);
  if (!fs.existsSync(harness)) {
    throw new Error(`E2E harness not built (${harness}); run scripts/run-all-e2e first`);
  }
  const network = process.env.GHOST_E2E_NETWORK || 'testnet-10';
  execFileSync(harness, ['fund', '--network', network, '--min-kas', '5', '--topup-kas', '10'], {
    stdio: 'inherit',
  });
}
