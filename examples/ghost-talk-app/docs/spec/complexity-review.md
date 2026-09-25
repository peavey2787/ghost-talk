# Complexity review exemptions

Normal design targets are **<50 lines per function** and structural complexity **<=6**. The hard ceilings remain 75 lines and CC 8. Every function crossing a normal target is listed here after SRP review; the architecture checker fails when a crossing is not recorded. Entries are not blanket waivers: any material change must be reviewed again, and a function must be split when it starts mixing responsibilities.

| Function | Current size | CC | Review |
| --- | ---: | ---: | --- |
| `crates/ghost-talk/src/receiver.rs::receive_chunk_at` | 46 | 7 | Reviewed as a bounded parser/validator/reassembly state machine; its branches represent distinct wire or buffer states and further extraction would obscure the state table. |
| `examples/ghost-talk-app/crates/ghost-protocol/src/protocol/canonical_json.rs::write_canonical_scalar` | 14 | 7 | Reviewed as a bounded parser/validator/reassembly state machine; its branches represent distinct wire or buffer states and further extraction would obscure the state table. |
| `examples/ghost-talk-app/crates/ghost-talk-native/src/hydra_commands/handshake/response.rs::prepare_first_message_finish` | 53 | 2 | Reviewed as a single HYDRA/KKTP protocol transition. The remaining branches are protocol-state outcomes and do not cross ownership boundaries. |
| `examples/ghost-talk-app/crates/ghost-talk-native/src/hydra_commands/session_types.rs::prepare_restart_successor` | 55 | 2 | Reviewed as a single HYDRA/KKTP protocol transition. The remaining branches are protocol-state outcomes and do not cross ownership boundaries. |
| `examples/ghost-talk-app/crates/ghost-wasm/src/app/startup.rs::attempt_automatic_unlock` | 35 | 7 | Reviewed as the automatic-unlock decision path; branches are terminal credential/runtime outcomes and no domain store ownership is bypassed. |
| `examples/ghost-talk-app/crates/ghost-wasm/src/components/call/presentation.rs::call_action_buttons` | 23 | 7 | Reviewed as cohesive call lifecycle/presentation orchestration. State mutation remains delegated to CallManager; the function contains only one call-stage responsibility. |
| `examples/ghost-talk-app/crates/ghost-wasm/src/components/call/transport.rs::send_existing_transport_request` | 27 | 7 | Reviewed as cohesive call lifecycle/presentation orchestration. State mutation remains delegated to CallManager; the function contains only one call-stage responsibility. |
| `examples/ghost-talk-app/crates/ghost-wasm/src/components/call/manager.rs::use_call_runtime` | 54 | 1 | Reviewed as the single Yew hook that assembles process-local call/Room-media handles. Domain mutation remains outside the hook and capture/Room transport logic is split into dedicated modules. |
