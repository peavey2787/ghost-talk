import type {
  ChatThread,
  Contact,
  Profile,
  ProfileBackupArchive,
  ProfileBackupContact,
  ProfileBackupMessage,
} from "./model";

export function backupContacts(profile: Profile): ProfileBackupContact[] {
  return profile.contacts.map(contact => ({
    id: contact.id,
    label: contact.label,
    kaspa_address: contact.kaspaAddress,
    hydra_handle: contact.hydraHandle,
  }));
}

export function backupMessages(profile: Profile): ProfileBackupMessage[] {
  if (!profile.settings.backupMessagesKaspa) return [];
  const output: ProfileBackupMessage[] = [];
  for (const chat of profile.chats) {
    for (const message of chat.messages) {
      // Kaspa-mailbox messages already have durable encrypted carrier data on chain.
      // The optional archive is only for messages whose original transport was not Kaspa.
      if (message.txid || message.direction === "system") continue;
      output.push({
        chat_id: chat.id,
        contact_id: chat.contactId,
        chat_label: chat.label,
        id: message.id,
        direction: message.direction,
        body: message.body,
        created_at: message.createdAt,
      });
    }
  }
  return output;
}

export async function profileBackupFingerprint(profile: Profile): Promise<string> {
  const bytes = new TextEncoder().encode(JSON.stringify({
    contacts: profile.settings.contactsBackupKaspa ? backupContacts(profile) : [],
    messages: backupMessages(profile),
  }));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
}

export function mergeRestoredArchive(profile: Profile, archive: ProfileBackupArchive): Profile {
  const contacts = mergeContacts(profile.contacts, archive.contacts);
  const chats = mergeMessages(profile.chats, archive, contacts);
  return { ...profile, contacts, chats };
}

function mergeContacts(current: Contact[], restored: ProfileBackupContact[]): Contact[] {
  const output = [...current];
  for (const candidate of restored) {
    const duplicate = output.find(contact =>
      contact.id === candidate.id
      || contact.kaspaAddress === candidate.kaspa_address
      || (candidate.hydra_handle && contact.hydraHandle === candidate.hydra_handle)
    );
    if (duplicate) continue;
    output.push({
      id: candidate.id,
      label: candidate.label,
      kaspaAddress: candidate.kaspa_address,
      hydraHandle: candidate.hydra_handle,
    });
  }
  return output;
}

function mergeMessages(
  current: ChatThread[],
  archive: ProfileBackupArchive,
  contacts: Contact[],
): ChatThread[] {
  let output = current.map(chat => ({ ...chat, messages: [...chat.messages] }));
  for (const restored of archive.messages) {
    const contact = restored.contact_id
      ? contacts.find(candidate => candidate.id === restored.contact_id)
      : undefined;
    let index = output.findIndex(chat => chat.id === restored.chat_id);
    if (index < 0 && contact) {
      index = output.findIndex(chat => chat.contactId === contact.id);
    }
    if (index < 0) {
      output.push({
        id: restored.chat_id,
        contactId: contact?.id ?? restored.contact_id,
        label: contact?.label ?? restored.chat_label,
        messages: [],
      });
      index = output.length - 1;
    }
    if (output[index].messages.some(message => message.id === restored.id)) continue;
    output[index].messages.push({
      id: restored.id,
      direction: restored.direction,
      body: restored.body,
      createdAt: restored.created_at,
    });
    output[index].messages.sort((a, b) => a.createdAt - b.createdAt);
  }
  return output;
}
