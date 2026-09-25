#[cfg(target_arch = "wasm32")]
pub(crate) use ghost_contacts::{Contact, ContactStore};
#[cfg(target_arch = "wasm32")]
pub use ghost_domain::call::{CallDirection, CallEvent, CallManager, CallPhase, CallRecord};

// Native unit tests exercise persistence round-trips only. Browser/application
// DTOs stay target-gated so native warnings-as-errors do not require dead-code
// suppression for code that cannot execute on that target.
#[cfg(all(not(target_arch = "wasm32"), test))]
pub(crate) use ghost_runtime::Profile;

#[cfg(target_arch = "wasm32")]
mod wallet_contact_message;
#[cfg(target_arch = "wasm32")]
pub(crate) use wallet_contact_message::{
    Deserialize, IncomingRequest, Message, Room, RoomBan, RoomInviteMeta, RoomMember, RoomMessage,
    RoomStore, RoomTombstone, RoomWire, Serialize, Settings, WalletProjection, WalletRecord,
};

#[cfg(target_arch = "wasm32")]
mod chat_room;
#[cfg(target_arch = "wasm32")]
pub(crate) use chat_room::{
    Chat, ChatStore, DirectoryLiveEvent, HydraContactAcceptedProjection,
    HydraIncomingRequestProjection, HydraReady, HydraRecoveryProjection, MailboxSendResult,
    NetworkStatusEvent, Profile, PublicGhostProfile, ReceivedProjection, ResolvedGhostPeer,
    WalletCreateResponse, WalletHistoryEntry, WalletHistoryResult, WalletImportResponse,
    WalletLiveEvent, WalletRecovery, WalletSnapshot,
};

#[cfg(target_arch = "wasm32")]
mod runtime;
#[cfg(target_arch = "wasm32")]
pub(crate) use runtime::{
    BackupContact, BackupMessage, BroadcastResult, CallRuntimeEvent, DebugLogEntry,
    DebugLogSnapshot, HydraCallSignalProjection, HydraControlProjection, HydraRealtimeEnvelope,
    HydraMailboxResult, HydraSessionBindingProjection, KasKoldBackupResult, PeerRouteRegistration, ProfileBackupPublishResult,
    ProfileBackupRestoreResult, PublishedGhostDescriptor, RealtimeControl,
};

#[cfg(target_arch = "wasm32")]
mod profile_patch;
#[cfg(target_arch = "wasm32")]
pub(crate) use profile_patch::{
    BroadcastDelta, ChatDelta, ContactDelta, ProfileDelta, ProfilePatch, RoomDelta,
    RoomTombstoneDelta,
};

#[cfg(target_arch = "wasm32")]
pub(crate) use ghost_runtime::WalletStateService;

#[cfg(target_arch = "wasm32")]
pub use ghost_chat::ReactionKind;
