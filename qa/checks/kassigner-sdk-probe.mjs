import fs from 'node:fs';
import assert from 'node:assert/strict';
import * as sdk from '../../external/kassigner-sdk/pkg/kassigner_sdk.js';

const wasmPath = new URL('../../external/kassigner-sdk/pkg/kassigner_sdk_bg.wasm', import.meta.url);
sdk.initSync({ module: fs.readFileSync(wasmPath) });

const requiredExports = [
  'KasSigner',
  'ProtocolQrDecoder',
  'kassigner_protocol_version',
  'kassigner_protocol_create_privacy_pairing_request',
  'kassigner_protocol_accept_privacy_pairing_response',
  'kassigner_protocol_decode_account',
  'kassigner_protocol_pskt_to_kspt',
  'kassigner_protocol_kspt_to_pskt',
  'kassigner_protocol_attach_input_derivation',
  'kassigner_protocol_attach_output_derivation',
  'kassigner_protocol_finalize_pskt',
];
for (const name of requiredExports) assert.ok(name in sdk, `missing KasSigner SDK export: ${name}`);

const expectedLimits = {
  ksptGeneration: 4,
  maxInputs: 32,
  maxOutputs: 8,
  maxScriptBytes: 512,
  maxRedeemScriptBytes: 1024,
  maxPayloadBytes: 768,
  maxSignaturesPerInput: 5,
  maxMultisigKeys: 5,
  maxMultisigWallets: 2,
  qrMaxFrames: 64,
  qrMultiFrameFragmentBytes: 91,
  qrSingleFramePayloadBytes: 134,
  qrSessionFrameVersion: 1,
  qrSessionBinding: true,
};

const signer = new sdk.KasSigner();
const decoder = new sdk.ProtocolQrDecoder();
try {
  assert.equal(signer.version, '2.0.0');
  assert.equal(sdk.kassigner_protocol_version(), '2.0.0');
  assert.deepEqual(JSON.parse(signer.limits()), expectedLimits);
  assert.deepEqual(JSON.parse(decoder.progress()), { total: 0, received: 0, bits: [] });
  const request = JSON.parse(sdk.kassigner_protocol_create_privacy_pairing_request('42'.repeat(16), 0, 3, 0, 2));
  assert.equal(request.nonceHex, '42'.repeat(16));
  assert.equal(request.receiveStart, 0);
  assert.equal(request.receiveCount, 3);
  assert.equal(request.changeStart, 0);
  assert.equal(request.changeCount, 2);
  assert.ok(Array.isArray(request.qrFrames) && request.qrFrames.length === 1);
  assert.equal(request.qrFrames[0].index, 0);
  assert.equal(request.qrFrames[0].total, 1);
} finally {
  decoder.free();
  signer.free();
}

console.log(JSON.stringify({ sdkVersion: '2.0.0', protocolVersion: '2.0.0', limits: expectedLimits, result: 'ok' }));
