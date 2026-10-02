// Ghost Talk UI driver shared by the two-instance scenarios. Every step goes
// through the real web application exactly as a user would.
import { expect } from '@playwright/test';
import { harnessOutput } from './instances.mjs';

export const PASSWORD = 'ghost-e2e-password';
const LONG = 5 * 60 * 1000;

export function wallets() {
  const raw = harnessOutput('GHOST_E2E_WALLETS');
  const line = raw.split(/\r?\n/).find((text) => text.includes('"wallets"'));
  return JSON.parse(line).wallets;
}

/** Restore a Ghost Talk ID from recovery words on testnet-10. */
export async function restoreIdentity(page, label, mnemonic) {
  await page.goto('/index.html');
  await page.getByRole('button', { name: 'Restore', exact: true }).click();
  await page.getByPlaceholder('Your local display name').fill(label);
  await page.getByPlaceholder('At least 8 characters').fill(PASSWORD);
  await page.locator('form.create-id select').first().selectOption('testnet-10');
  await page.getByPlaceholder('word1 word2 …').fill(mnemonic);
  await page.getByRole('button', { name: 'Restore Ghost Talk ID' }).click();
  await expect(page.locator('.sidebar nav')).toBeVisible({ timeout: LONG });
  await expect(page.locator('.network-dot.connected')).toBeVisible({ timeout: LONG });
}

export async function nav(page, name) {
  await page.locator('.sidebar nav button', { hasText: name }).first().click();
}

async function selectSetting(page, label, value) {
  await nav(page, 'Settings');
  await page.locator('label', { hasText: label }).locator('select').first().selectOption(value);
}

/** Realtime route: "Auto" or "Kaspa only". */
export const setRoute = (page, value) => selectSetting(page, 'Realtime route', value);
/** Text messages: "Kaspa", "P2P preferred", or "P2P only". */
export const setTextRoute = (page, value) => selectSetting(page, 'Text messages', value);

/** Point p2p-net at the local relay and turn on protocol debugging. */
export async function configureNetwork(page, relays) {
  await nav(page, 'Settings');
  const list = relays.join('\n');
  for (const selector of ['textarea.p2p-relay-peers', 'textarea.p2p-bootstrap-peers']) {
    await page.locator(selector).fill(list);
    await page.locator(selector).blur();
  }
  const debug = page.locator('.debug-settings-card input[type=checkbox]');
  if (!(await debug.isChecked())) await debug.check();
}

export async function openChatWith(page, address) {
  await nav(page, 'Chats');
  const existing = page.locator('.thread-row').first();
  if (await existing.count()) {
    await existing.click();
    return;
  }
  await page.locator('.compact-start input').first().fill(address);
  await page.locator('.compact-start button.primary').click();
  await expect(page.locator('.chat')).toBeVisible({ timeout: LONG });
}

export async function sendChat(page, body) {
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill(body);
  await page.locator('.composer button[type=submit]').click();
}

export async function expectChatMessage(page, body) {
  await nav(page, 'Chats');
  const row = page.locator('.thread-row').first();
  await expect(row).toBeVisible({ timeout: LONG });
  await row.click();
  const message = page.locator('.messages p', { hasText: body });
  await expect(message).toBeVisible({ timeout: LONG });
  return message;
}

export async function acceptChatRequest(page) {
  await nav(page, 'Chats');
  await page.locator('.thread-row').first().click({ timeout: LONG });
  const banner = page.locator('.chat-request-banner');
  await expect(banner).toBeVisible({ timeout: LONG });
  await banner.getByRole('button', { name: 'Accept' }).click();
}

export async function startCall(page) {
  await nav(page, 'Chats');
  await page.locator('.thread-row').first().click();
  const button = page.locator('button.call-button');
  await expect(button).toBeEnabled({ timeout: LONG });
  await button.click();
}

export async function acceptIncomingCall(page) {
  const modal = page.locator('.voice-call-modal');
  await expect(modal.getByText('Incoming call')).toBeVisible({ timeout: LONG });
  await modal.getByRole('button', { name: 'Accept' }).click();
}

export async function expectCallConnected(page) {
  await expect(page.locator('.voice-call-modal .call-state', { hasText: 'Connected' })).toBeVisible({ timeout: LONG });
}

export async function hangUp(page) {
  const modal = page.locator('.voice-call-modal');
  const hangup = modal.getByRole('button', { name: 'Hang up' });
  if (await hangup.count()) await hangup.click();
  await page.getByRole('button', { name: 'Close' }).click({ timeout: 10_000 }).catch(() => {});
}

/** Both sides must leave the call: no call window may remain open. */
export async function expectNoCall(page) {
  await expect(page.locator('.voice-call-modal')).toHaveCount(0, { timeout: LONG });
}

export async function routeLabel(page) {
  return page.locator('.voice-call-modal .call-route').innerText();
}

/** Protocol-debug log text (redacted metadata only). */
export async function debugLog(page) {
  await nav(page, 'Settings');
  await page.getByRole('button', { name: 'Open debug window' }).click();
  const window = page.locator('.debug-window');
  await window.getByRole('button', { name: 'Refresh' }).click();
  await page.waitForTimeout(500);
  const entries = await window.locator('.debug-log-entry').allInnerTexts();
  await window.getByRole('button', { name: 'Close', exact: true }).click();
  return entries.map((entry) => entry.replace(/\s+/g, ' '));
}

/** Wait until the debug log contains every fragment on one entry. */
export async function expectDebug(page, ...fragments) {
  await expect
    .poll(async () => {
      const entries = await debugLog(page);
      return entries.some((entry) => fragments.every((fragment) => entry.includes(fragment)));
    }, { timeout: LONG, intervals: [3000] })
    .toBe(true);
}
