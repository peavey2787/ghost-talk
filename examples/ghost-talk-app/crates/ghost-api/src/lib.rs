#![forbid(unsafe_code)]

mod backup;
mod capabilities;
mod events;
mod hydra;
mod kaskold;
mod media;
mod metadata;
mod peer;
mod wallet;

pub use backup::{
    validate_profile_backup_inputs, BackupContact, BackupMessage, ProfileBackupArchive, ProfileBackupPublishResult,
    ProfileBackupRestoreResult,
};
pub use capabilities::{
    default_capabilities, select_conversation_mode, ConversationMode, ProtocolAvailability,
    GHOST_AVATAR_V1, GHOST_BROADCAST_V1, GHOST_MEDIA_V1, GHOST_PROFILE_V2, GHOST_ROOM_VOICE_V1,
    HYDRA_PQ, KASIA_V1, KASPA_MAILBOX, KKTP_V2, PRIVATE_BOOTSTRAP,
};
pub use events::{
    DebugLogEntry, DebugLogSnapshot, DirectoryLiveEvent, MailboxEvent, NetworkStatusEvent,
    WalletLiveEvent,
};
pub use hydra::{
    HydraCallSignalProjection, HydraContactAcceptedProjection, HydraControlProjection,
    HydraIncomingRequestProjection, HydraMailboxResult, HydraReady,
    HydraRealtimeEnvelope,
    HydraRecoveryProjection, HydraRoomInviteProjection, HydraSessionBindingProjection,
    HydraSessionEndedProjection, ReceivedProjection,
};
pub use metadata::{app_info, AppInfo};
pub use media::{
    BroadcastSinkFailure, BroadcastStartRequest, BroadcastStopResult, KaspaArchivePlan,
    KaspaArchivePublishResult, VerifiedMedia,
};
pub use peer::{PeerRouteRegistration, PublicGhostProfile, ResolvedGhostPeer};
pub use wallet::{
    derivation_presets, BroadcastResult, DerivationPresetInfo, MailboxSendResult, PublishedGhostDescriptor, WalletCreateResponse,
    WalletHistoryEntry, WalletHistoryResult, WalletImportResponse, WalletProjection,
    WalletRecovery, WalletSnapshot,
};

pub use kaskold::{
    KasKoldBackupResult, KasKoldInventoryResult, KasKoldReviewOutput, KasKoldReviewResult,
    KasKoldSignResult, KasKoldWalletSummary,
};
