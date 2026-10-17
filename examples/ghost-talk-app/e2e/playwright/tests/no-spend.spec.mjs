// Two real Ghost Talk web instances in No-spend mode: everything travels over
// the p2p-net swarm, nothing is broadcast to Kaspa.
//  1. a chat request and replies both ways
//  2. retention: a message sent while the receiver is closed is recovered
//     from the swarm when the receiver comes back
//  3. a public room is listed in Discover over the swarm
import { test, expect } from '@playwright/test';
import { launchInstance, relayAddresses } from './support/instances.mjs';
import * as app from './support/app.mjs';

test.describe.configure({ mode: 'serial' });

let alice;
let bob;
const stamp = `ns-${Date.now().toString(36)}`;

test.beforeAll(async ({ browser }) => {
  const wallets = app.wallets();
  const relays = relayAddresses();
  alice = await launchInstance(browser, 'alice-no-spend');
  bob = await launchInstance(browser, 'bob-no-spend');
  alice.address = wallets.instanceA.address;
  bob.address = wallets.instanceB.address;
  await Promise.all([
    app.restoreIdentity(alice.page, 'Alice No-spend', wallets.instanceA.mnemonic),
    app.restoreIdentity(bob.page, 'Bob No-spend', wallets.instanceB.mnemonic),
  ]);
  for (const instance of [alice, bob]) {
    await app.configureNetwork(instance.page, relays);
    await app.setSpendMode(instance.page, 'No-spend');
  }
});

test.afterEach(async ({}, testInfo) => {
  if (testInfo.status === testInfo.expectedStatus) return;
  for (const instance of [alice, bob]) {
    const entries = await app.debugLog(instance.page).catch((error) => [`debug log unavailable: ${error}`]);
    await testInfo.attach(`${instance.name}-debug-log`, { body: entries.join('\n'), contentType: 'text/plain' });
  }
});

test.afterAll(async () => {
  await alice?.context.close();
  await bob?.context.close();
});

test('1. No-spend chat: request and replies travel over the swarm', async () => {
  await app.openChatWith(alice.page, bob.address);
  await app.sendChat(alice.page, `swarm hello ${stamp}`);
  await app.acceptChatRequest(bob.page);
  await app.expectChatMessage(bob.page, `swarm hello ${stamp}`);
  await app.sendChat(bob.page, `swarm reply ${stamp}`);
  await app.expectChatMessage(alice.page, `swarm reply ${stamp}`);
});

test('2. Retention: mail sent while the receiver was closed is recovered from the swarm', async () => {
  await bob.page.goto('about:blank');
  await app.openChatWith(alice.page, bob.address);
  await app.sendChat(alice.page, `while you were away ${stamp}`);
  await alice.page.waitForTimeout(5_000);
  await app.reopen(bob.page);
  await app.expectChatMessage(bob.page, `while you were away ${stamp}`);
});

test('3. A public room is listed in Discover over the swarm', async () => {
  const roomName = `Swarm Room ${stamp}`;
  await app.createPublicRoom(alice.page, roomName);
  const row = await app.expectListedRoom(bob.page, roomName);
  await expect(row.getByRole('button', { name: 'Message host' })).toBeVisible();
  const own = await app.expectListedRoom(alice.page, roomName);
  await expect(own.getByRole('button', { name: 'Announce on Kaspa' })).toBeDisabled();
});
