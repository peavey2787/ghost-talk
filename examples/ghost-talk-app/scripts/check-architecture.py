#!/usr/bin/env python3
"""Ghost Talk repository architecture, SRP, and complexity guardrails.

The executable is deliberately thin. Focused checks live under
`scripts/architecture/`; coverage-dependent CRAP remains in check-crap.py.
"""
from __future__ import annotations

import sys

from architecture.common import (
    MAX_COMPLEXITY, MAX_FUNCTION_LINES, NORMAL_FILE_LINES, NORMAL_FUNCTION_LINES,
    NORMAL_COMPLEXITY, errors, production_rust_files, function_bodies,
    structural_complexity, sanitize_rust,
)
from architecture.source_structure import (
    check_repository_shape, check_real_module_resolution, check_local_reexport_symbols,
    check_exact_function_duplication, check_implicit_sibling_imports,
    check_sizes_and_complexity, check_duplicate_domain_types, check_quality_script_shape,
)
from architecture.ownership import (
    check_profile_domain_boundaries, check_strict_ownership_surface,
    check_lifecycle_mutation_guards, check_controller_event_and_browser_regressions,
    check_shared_model_usage, check_duplicate_struct_schemas, check_contact_binding_privacy,
)
from architecture.runtime_checks import (
    check_call_ownership, check_identity_ownership, check_mailbox_ownership,
    check_transport_ownership, check_wallet_progress, check_obvious_dead_crate_api,
    check_obsolete_code, check_rust_surface_sanity, check_storage_root_ownership,
    check_native_instance_lock_ownership,
)
from architecture.integration_checks import (
    check_dotk_integration, check_native_frontend_contract, check_versions, check_release_engineering,
)
from architecture.dependency_checks import (
    check_declared_first_party_usage, check_first_party_dependencies, check_local_lock_consistency,
    check_kasia_wasm_portability, check_mailbox_submission_ownership, check_yew_indexmap_unification,
)
from architecture.mobile_checks import check_mobile_application_contracts
from architecture.parity_checks import (
    check_cross_platform_command_parity, check_broadcast_transport_parity, check_shared_parity_owners,
    check_browser_host_compile_contracts,
)
from architecture.expansion_checks import (
    check_identity_media_room_contracts, check_ghost_kasia_indexer_boundary,
    check_media_broadcast_interop_completion, check_message_reaction_contracts,
    check_kaskold_compatibility_and_ui_contracts, check_crap_safe_zero_coverage_boundaries,
)


def main() -> int:
    files = production_rust_files()
    if not files:
        from architecture.common import fail
        fail("no production Rust files found")
    check_repository_shape(files)
    check_quality_script_shape()
    check_real_module_resolution(files)
    check_first_party_dependencies()
    check_declared_first_party_usage()
    check_local_lock_consistency()
    check_yew_indexmap_unification()
    check_kasia_wasm_portability()
    check_mailbox_submission_ownership()
    check_local_reexport_symbols(files)
    check_obvious_dead_crate_api(files)
    check_exact_function_duplication(files)
    check_implicit_sibling_imports(files)
    check_sizes_and_complexity(files)
    check_duplicate_domain_types(files)
    check_duplicate_struct_schemas(files)
    check_contact_binding_privacy()
    check_profile_domain_boundaries(files)
    check_strict_ownership_surface(files)
    check_lifecycle_mutation_guards(files)
    check_controller_event_and_browser_regressions()
    check_shared_model_usage()
    check_call_ownership(files)
    check_identity_ownership()
    check_mailbox_ownership()
    check_transport_ownership(files)
    check_wallet_progress(files)
    check_obsolete_code(files)
    check_rust_surface_sanity(files)
    check_storage_root_ownership()
    check_native_instance_lock_ownership()
    check_dotk_integration()
    check_native_frontend_contract()
    check_identity_media_room_contracts()
    check_ghost_kasia_indexer_boundary()
    check_media_broadcast_interop_completion()
    check_message_reaction_contracts()
    check_kaskold_compatibility_and_ui_contracts()
    check_crap_safe_zero_coverage_boundaries()
    check_mobile_application_contracts()
    check_cross_platform_command_parity()
    check_broadcast_transport_parity()
    check_shared_parity_owners()
    check_browser_host_compile_contracts()
    check_release_engineering()
    check_versions()

    if errors:
        print("Ghost Talk architecture/SRP guardrails: FAIL", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print("PASS: repository-wide architecture/SRP guardrails")
    print(f"PASS: all Rust source files < {NORMAL_FILE_LINES} lines")
    print(f"PASS: first-party QA scripts < {NORMAL_FILE_LINES} lines with cohesive names")
    print(f"PASS: all Rust functions <= {MAX_FUNCTION_LINES} lines and structural complexity <= {MAX_COMPLEXITY}")
    print(f"PASS: functions >= {NORMAL_FUNCTION_LINES} lines or complexity > {NORMAL_COMPLEXITY} have explicit SRP review records")
    print("PASS: real semantic modules only; no include!/numeric parts/#[path] aliases")
    print("PASS: first-party path dependencies are consumed and dead library crates are forbidden")
    print("PASS: outbound mailbox unlock/fee/submission setup has one owner")
    print("PASS: live calls are process-local and owned by ghost-domain CallManager")
    print("PASS: canonical PeerBinding + ContactService own authenticated peer identity resolution")
    print("PASS: browser chat/call controls and deterministic two-instance ownership regressions are present")
    print("PASS: MailboxService owns durable mailbox envelopes; packet failures are isolated per envelope")
    print("PASS: HydraSessionManager owns chat-session projection and NodeSession owns wallet/transport Kaspa endpoint selection")
    print("PASS: WalletStateService owns mutable wallet state and progress merges are monotonic")
    print("PASS: runtime storage and HYDRA profile leases have single owners")
    print("PASS: DotK resolution pins the deployment and requires local derivation + live covenant proof")
    print("PASS: unified names, shared media, RoomAccess, and Ghost/Kasia indexer boundaries are enforced")
    print("PASS: Room voice convergence, live fan-out, RTMP/recording, archive-resume, and no-downgrade contracts are enforced")
    print("PASS: authenticated direct/Room message reaction ownership and concurrency contracts are enforced")
    print("PASS: KasKold 2.0 import/backup/signing and chat/Rooms UI compatibility contracts are enforced")
    print("PASS: known zero-coverage CRAP boundaries remain CC<=4")
    print("PASS: Android/iOS shared-UX mobile shells and self-bootstrap contracts are enforced")
    print("PASS: downstream security/contribution/CI/release-engineering surfaces are enforced")
    print("PASS: Ghost Talk package versions remain 0.1.0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
