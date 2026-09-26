// Shared helpers: isolated browser instances and harness-provided settings.
import fs from 'node:fs';

/** Harness output passed either inline or as a file path (`<NAME>_FILE`). */
export function harnessOutput(name) {
  const file = process.env[`${name}_FILE`];
  if (file) return fs.readFileSync(file, 'utf8');
  const raw = process.env[name];
  if (!raw) throw new Error(`${name} is not set (run through scripts/run-all-e2e)`);
  return raw;
}

export function relayAddresses() {
  const raw = harnessOutput('GHOST_E2E_RELAY');
  // The relay also prints OS tool output (key-file ACL hardening); take the JSON line.
  const line = raw
    .split(/\r?\n/)
    .find((text) => text.trim().startsWith('{') && text.includes('peerId'));
  if (!line) throw new Error(`relay announcement not found in: ${raw}`);
  return JSON.parse(line).addresses;
}

/** A separate browser context = a separate Ghost Talk installation. */
export async function launchInstance(browser, name) {
  const context = await browser.newContext({
    permissions: ['microphone'],
    viewport: { width: 1280, height: 860 },
  });
  const page = await context.newPage();
  page.on('console', (message) => {
    if (message.type() === 'error') console.log(`[${name}] console error: ${message.text()}`);
  });
  return { name, context, page };
}
