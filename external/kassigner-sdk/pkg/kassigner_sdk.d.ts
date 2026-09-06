/* tslint:disable */
/* eslint-disable */

export class KasSigner {
    free(): void;
    [Symbol.dispose](): void;
    acceptQrFrame(frame_hex: string): string | undefined;
    accountFingerprint(): string | undefined;
    complete(request_json: string, response_hex: string): string;
    createPrivacyPairingRequest(receive_start: number, receive_count: number, change_start: number, change_count: number): string;
    finalize(signed_json: string): string;
    limits(): string;
    constructor();
    pairNormal(account_payload: string, network: string): string;
    pairPrivacy(response_hex: string, network: string): string;
    prepare(pskt_hex: string, network: string): string;
    qrDecoderProgress(): string;
    resetQrDecoder(): void;
    readonly version: string;
}

export class ProtocolQrDecoder {
    free(): void;
    [Symbol.dispose](): void;
    accept(frame_hex: string): string | undefined;
    constructor();
    progress(): string;
    reset(): void;
}

export function kassigner_protocol_accept_privacy_pairing_response(request_json: string, response_hex: string, network: string, expected_account_fingerprint?: string | null): string;

export function kassigner_protocol_attach_input_derivation(pskt_hex: string, input_index: number, branch: number, index: number): string;

export function kassigner_protocol_attach_output_derivation(pskt_hex: string, output_index: number, branch: number, index: number): string;

export function kassigner_protocol_create_privacy_pairing_request(nonce_hex: string, receive_start: number, receive_count: number, change_start: number, change_count: number): string;

export function kassigner_protocol_decode_account(account_payload: string, network: string): string;

export function kassigner_protocol_finalize_pskt(pskt_hex: string): string;

export function kassigner_protocol_kspt_to_pskt(original_pskt_hex: string, signed_kspt_hex: string, network: string): string;

export function kassigner_protocol_pskt_to_kspt(pskt_hex: string, network: string): string;

export function kassigner_protocol_version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_kassigner_free: (a: number, b: number) => void;
    readonly kassigner_acceptQrFrame: (a: number, b: number, c: number) => [number, number, number, number];
    readonly kassigner_accountFingerprint: (a: number) => [number, number];
    readonly kassigner_complete: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kassigner_createPrivacyPairingRequest: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kassigner_finalize: (a: number, b: number, c: number) => [number, number, number, number];
    readonly kassigner_limits: (a: number) => [number, number, number, number];
    readonly kassigner_new: () => number;
    readonly kassigner_pairNormal: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kassigner_pairPrivacy: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kassigner_prepare: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kassigner_qrDecoderProgress: (a: number) => [number, number, number, number];
    readonly kassigner_resetQrDecoder: (a: number) => void;
    readonly kassigner_version: (a: number) => [number, number];
    readonly __wbg_protocolqrdecoder_free: (a: number, b: number) => void;
    readonly kassigner_protocol_accept_privacy_pairing_response: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number, number];
    readonly kassigner_protocol_attach_input_derivation: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kassigner_protocol_attach_output_derivation: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kassigner_protocol_create_privacy_pairing_request: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly kassigner_protocol_decode_account: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly kassigner_protocol_finalize_pskt: (a: number, b: number) => [number, number, number, number];
    readonly kassigner_protocol_kspt_to_pskt: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly kassigner_protocol_pskt_to_kspt: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly kassigner_protocol_version: () => [number, number];
    readonly protocolqrdecoder_accept: (a: number, b: number, c: number) => [number, number, number, number];
    readonly protocolqrdecoder_new: () => number;
    readonly protocolqrdecoder_progress: (a: number) => [number, number, number, number];
    readonly protocolqrdecoder_reset: (a: number) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
