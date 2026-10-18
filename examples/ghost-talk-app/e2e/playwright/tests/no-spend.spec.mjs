// Two real Ghost Talk web instances in No-spend mode: everything travels over
// the p2p-net swarm, nothing is broadcast to Kaspa.
//  1. retention: a chat request sent while the receiver is closed is
//     recovered from the swarm when the receiver comes back
//  2. replies travel both ways over the swarm
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

test.afterEach(async ({}, testInfo) => app.dumpDebugOnFailure(testInfo, [alice, bob]));

test.afterAll(async () => {
  await alice?.context.close();
  await bob?.context.close();
});

test('1. Retention: a chat request sent while the receiver was closed is recovered from the swarm', async () => {
  // A request needs no live session, so it is the mail a restarted receiver
  // can always open; session mail waits for the initiator to resume.
  await bob.page.goto('about:blank');
  await app.openChatWith(alice.page, bob.address);
  await app.sendChat(alice.page, `while you were away ${stamp}`);
  await alice.page.waitForTimeout(5_000);
  await app.reopen(bob.page);
  await app.acceptChatRequest(bob.page);
  await app.expectChatMessage(bob.page, `while you were away ${stamp}`);
});

test('2. No-spend chat: replies travel both ways over the swarm', async () => {
  await app.sendChat(bob.page, `swarm reply ${stamp}`);
  await app.expectChatMessage(alice.page, `swarm reply ${stamp}`);
  await app.sendChat(alice.page, `swarm hello ${stamp}`);
  await app.expectChatMessage(bob.page, `swarm hello ${stamp}`);
});

test('3. A public room is listed in Discover over the swarm', async () => {
  const roomName = `Swarm Room ${stamp}`;
  await app.createPublicRoom(alice.page, roomName);
  const row = await app.expectListedRoom(bob.page, roomName);
  await expect(row.getByRole('button', { name: 'Message host' })).toBeVisible();
  const own = await app.expectListedRoom(alice.page, roomName);
  await expect(own.getByRole('button', { name: 'Announce on Kaspa' })).toBeDisabled();
});
