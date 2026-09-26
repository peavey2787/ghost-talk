use ghost_api::{
    KasKoldBackupResult, KasKoldInventoryResult, KasKoldReviewResult, KasKoldSignResult,
};

#[tauri::command]
pub async fn kaskold_import_text(
    password: String,
    sealed_inventory: Vec<u8>,
    kind: String,
    value: String,
    passphrase: String,
) -> Result<KasKoldInventoryResult, String> {
    crate::run_blocking("KasKold text import", move || {
        ghost_kaskold::import_text_inventory(
            &password,
            &sealed_inventory,
            &kind,
            &value,
            &passphrase,
        )
    })
    .await
}

#[tauri::command]
pub async fn kaskold_import_bytes(
    password: String,
    sealed_inventory: Vec<u8>,
    kind: String,
    data: Vec<u8>,
    credential: String,
) -> Result<KasKoldInventoryResult, String> {
    crate::run_blocking("KasKold file import", move || {
        ghost_kaskold::import_bytes_inventory(
            &password,
            &sealed_inventory,
            &kind,
            &data,
            &credential,
        )
    })
    .await
}

#[tauri::command]
pub async fn kaskold_backup(
    password: String,
    sealed_inventory: Vec<u8>,
    kind: String,
    credential: String,
    carrier: Vec<u8>,
) -> Result<KasKoldBackupResult, String> {
    crate::run_blocking("KasKold backup", move || {
        ghost_kaskold::backup_inventory(&password, &sealed_inventory, &kind, &credential, &carrier)
    })
    .await
}

#[tauri::command]
pub async fn kaskold_review_pskt(
    password: String,
    sealed_inventory: Vec<u8>,
    pskt_hex: String,
    network: String,
) -> Result<KasKoldReviewResult, String> {
    crate::run_blocking("KasKold PSKT review", move || {
        ghost_kaskold::review_pskt(&password, &sealed_inventory, &pskt_hex, &network)
    })
    .await
}

#[tauri::command]
pub async fn kaskold_sign_pskt(
    password: String,
    sealed_inventory: Vec<u8>,
    pskt_hex: String,
    network: String,
    review_token: String,
) -> Result<KasKoldSignResult, String> {
    crate::run_blocking("KasKold PSKT signing", move || {
        ghost_kaskold::sign_pskt(
            &password,
            &sealed_inventory,
            &pskt_hex,
            &network,
            &review_token,
        )
    })
    .await
}
