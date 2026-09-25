# Schema duplication exemptions

The architecture checker rejects first-party structs with materially identical field names and field types unless the duplication is listed here and carries a domain/security reason.

## `WalletSecret` / `WalletRecovery`

These types intentionally have the same serialized fields but different security lifecycles. `ghost-kaspa::WalletSecret` is signing material and implements zeroization (`Zeroize`/`ZeroizeOnDrop`). `ghost-api::WalletRecovery` is a short-lived native/WASM response DTO used only to render an explicitly requested recovery export. Keeping the API projection out of the signing type prevents UI/native contract crates from importing wallet signing internals or accidentally extending the lifetime/traits of the secret-bearing wallet type.

## `WalletPublic` / `WalletProjection`

These types intentionally share a serialized shape across a trust boundary. `ghost-kaspa::WalletPublic` belongs to the signing/derivation subsystem and includes wallet-specific behavior such as portal conversion and derivation advancement. `ghost-domain::WalletProjection` is the durable/UI projection and exposes only monotonic progress merging. Keeping the projection separate prevents frontend/runtime crates from acquiring signing-oriented wallet behavior while retaining an explicit conversion boundary.

No other identical first-party struct schemas are exempt.
