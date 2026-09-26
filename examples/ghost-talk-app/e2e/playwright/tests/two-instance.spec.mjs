// Two real Ghost Talk web instances on Kaspa testnet-10:
//  1. direct chat: text + voice call
//  2. Auto: Kaspa carries the signed transport announcement, then text and
//     voice move to p2p-net (WebRTC-direct via the relay); p2p text is marked
//     "not stored on Kaspa"
//  3. Kaspa only: text + voice entirely on Kaspa
//  4. Rooms: text + Room voice
import { test, expect } from '@playwright/test';
import { launchInstance, relayAddresses } from './support/instances.mjs';
import * as app from './support/app.mjs';

test.describe.configure({ mode: 'serial' });

let alice;
let bob;
const stamp = Date.now().toString(36);
const VOICE_SECONDS = 8_000;

test.beforeAll(async ({ browser }) => {
  const wallets = app.wallets();
  const relays = relayAddresses();
  alice = await launchInstance(browser, 'alice');
  bob = await launchInstance(browser, 'bob');
  alice.address = wallets.instanceA.address;
  bob.address = wallets.instanceB.address;
  await Promise.all([
    app.restoreIdentity(alice.page, 'Alice E2E', wallets.instanceA.mnemonic),
    app.restoreIdentity(bob.page, 'Bob E2E', wallets.instanceB.mnemonic),
  ]);
  for (const instance of [alice, bob]) {
    await app.configureNetwork(instance.page, relays);
    await app.setRoute(instance.page, 'Auto');
    await app.setTextRoute(instance.page, 'Kaspa');
  }
});

test.afterEach(async ({}, testInfo) => {
  if (testInfo.status === testInfo.expectedStatus) return;
  for (const instance of [alice, bob]) {
    const entries = await app.debugLog(instance.page).catch((error) => [`debug log unavailable: ${error}`]);
    await testInfo.attach(`${instance.name}-debug-log`, { body: entries.join('\n'), contentType: 'text/plain' });
    console.log(`==== ${instance.name} protocol debug`);
    for (const entry of entries.filter((e) => !e.includes('live-block')).slice(-150)) console.log(`  ${entry.slice(0, 300)}`);
  }
});

test.afterAll(async () => {
  await alice?.context.close();
  await bob?.context.close();
});

async function voiceCall(caller, callee) {
  await app.startCall(caller.page);
  await app.acceptIncomingCall(callee.page);
  await app.expectCallConnected(caller.page);
  await app.expectCallConnected(callee.page);
  await caller.page.waitForTimeout(VOICE_SECONDS);
}

test('1. direct chat: text both ways and a voice call', async () => {
  await app.openChatWith(alice.page, bob.address);
  await app.sendChat(alice.page, `hello bob ${stamp}`);
  await app.acceptChatRequest(bob.page);
  await app.expectChatMessage(bob.page, `hello bob ${stamp}`);
  await app.sendChat(bob.page, `hi alice ${stamp}`);
  await app.expectChatMessage(alice.page, `hi alice ${stamp}`);

  await voiceCall(alice, bob);
  await app.expectDebug(bob.page, 'realtime-received', 'kind=call-voice');
  await app.expectDebug(alice.page, 'realtime-received', 'kind=call-voice');
  await app.hangUp(alice.page);
  await app.hangUp(bob.page);
});

test('2. Auto: Kaspa signals the p2p-net route, then voice and text use p2p-net', async () => {
  // The signed transport announcement travels over Kaspa (the signal layer).
  await app.expectDebug(bob.page, 'announce-received');
  await app.expectDebug(alice.page, 'announce-ack-received');

  await voiceCall(alice, bob);
  await expect.poll(() => app.routeLabel(alice.page), { timeout: 120_000 }).toBe('P2P connected');
  await app.expectDebug(bob.page, 'realtime-received', 'carrier=p2p-net', 'kind=call-voice');
  await app.expectDebug(alice.page, 'realtime-received', 'carrier=p2p-net', 'kind=call-voice');
  await app.hangUp(alice.page);
  await app.hangUp(bob.page);

  for (const instance of [alice, bob]) await app.setTextRoute(instance.page, 'P2P only');
  await app.openChatWith(alice.page, bob.address);
  await app.sendChat(alice.page, `p2p text ${stamp}`);
  const received = await app.expectChatMessage(bob.page, `p2p text ${stamp}`);
  await expect(received.locator('.message-carrier')).toHaveText('p2p · not stored on Kaspa');
  await app.expectDebug(alice.page, 'realtime-sent', 'carrier=p2p-net', 'kind=direct-text');
  for (const instance of [alice, bob]) await app.setTextRoute(instance.page, 'Kaspa');
});

test('3. Kaspa only: text and voice travel entirely over Kaspa', async () => {
  for (const instance of [alice, bob]) await app.setRoute(instance.page, 'Kaspa only');
  await app.openChatWith(bob.page, alice.address);
  await app.sendChat(bob.page, `kaspa only ${stamp}`);
  const received = await app.expectChatMessage(alice.page, `kaspa only ${stamp}`);
  await expect(received.locator('.message-carrier')).toHaveCount(0);

  await voiceCall(bob, alice);
  expect(await app.routeLabel(bob.page)).toBe('Kaspa fallback');
  await app.expectDebug(alice.page, 'realtime-received', 'carrier=kaspa', 'kind=call-voice');
  await app.expectDebug(bob.page, 'realtime-sent', 'carrier=kaspa', 'kind=call-voice');
  await app.hangUp(bob.page);
  await app.hangUp(alice.page);
  for (const instance of [alice, bob]) await app.setRoute(instance.page, 'Auto');
});

test('4. Rooms: text both ways and Room voice', async () => {
  const roomName = `E2E Room ${stamp}`;
  await app.nav(alice.page, 'Rooms');
  await alice.page.getByPlaceholder('Room name').fill(roomName);
  await alice.page.locator('.room-create-form select').selectOption('community');
  await alice.page.locator('.room-create-form button.primary').click();
  await alice.page.getByPlaceholder('Contact, KNS, dot.k, or Kaspa address').fill(bob.address);
  await alice.page.getByRole('button', { name: 'Invite', exact: true }).click();

  await app.nav(bob.page, 'Rooms');
  const invite = bob.page.locator('.room-invite-notice', { hasText: roomName });
  await expect(invite).toBeVisible({ timeout: 5 * 60 * 1000 });
  await invite.getByRole('button', { name: 'Accept' }).click();

  for (const [sender, receiver, body] of [
    [alice, bob, `room hello ${stamp}`],
    [bob, alice, `room reply ${stamp}`],
  ]) {
    await app.nav(sender.page, 'Rooms');
    await sender.page.getByPlaceholder('Message room').fill(body);
    await sender.page.locator('.room-composer button.primary').click();
    await app.nav(receiver.page, 'Rooms');
    await expect(receiver.page.locator('.room-message', { hasText: body })).toBeVisible({ timeout: 5 * 60 * 1000 });
  }

  for (const instance of [alice, bob]) {
    await app.nav(instance.page, 'Rooms');
    await instance.page.getByRole('button', { name: 'Join voice' }).click();
    await expect(instance.page.locator('.room-voice.active')).toBeVisible();
  }
  await alice.page.waitForTimeout(VOICE_SECONDS);
  await app.expectDebug(bob.page, 'realtime-received', 'kind=room-voice');
  await app.expectDebug(alice.page, 'realtime-received', 'kind=room-voice');
  for (const instance of [alice, bob]) {
    await app.nav(instance.page, 'Rooms');
    await instance.page.getByRole('button', { name: 'Leave voice' }).click();
  }
});
