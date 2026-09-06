/**
 * confirmationWatcher.js - Wait for a tx to appear in a block; return queue position
 * Single responsibility: watch for RTT tx confirmation via scanner match; no send.
 */

import { Logger, LogModule } from "../../core/logger.js";

const log = Logger.create(LogModule.voice.rtt);

/**
 * Resolve txIndexInBlock from raw block and tx id.
 * @param {Object} rawBlock - Block with .transactions (WASM or JS)
 * @param {string} txId
 * @returns {number} Index or -1
 */
function getTxIndexInBlock(rawBlock, txId) {
  const txs = rawBlock?.transactions;
  if (!txs) return -1;
  let idx = 0;
  try {
    for (const tx of txs) {
      const id = tx.verboseData?.transactionId ?? tx.transactionId;
      if (String(id) === String(txId)) return idx;
      idx++;
    }
  } catch (e) {
    log.warn("confirmationWatcher: getTxIndexInBlock", e?.message ?? e);
  }
  return -1;
}

/**
 * Wait for tx to be confirmed (appear in a block) via scanner match.
 * Caller must ensure scanner has RTT prefix and is started.
 * @param {Object} portal - KaspaPortal (with intelligence.scanner)
 * @param {string} txId - Transaction id to watch
 * @param {number} timeoutMs
 * @returns {Promise<{ blockHash: string, blueScore: number, txIndexInBlock: number }>}
 */
export function waitForConfirmation(portal, txId, timeoutMs) {
  return new Promise((resolve, reject) => {
    const scanner = portal?.intelligence?.scanner;
    if (!scanner) {
      reject(new Error("ConfirmationWatcher: portal.intelligence.scanner required"));
      return;
    }

    const timeout = setTimeout(() => {
      unsubscribe();
      reject(new Error(`RTT confirmation timeout after ${timeoutMs}ms`));
    }, timeoutMs);

    const onMatch = (rawBlock, matchObj) => {
      const matchTxId = matchObj?.txid ?? matchObj?.transactionId;
      if (String(matchTxId) !== String(txId)) return;

      const blueScore = matchObj?.blueScore ?? rawBlock?.header?.blueScore;
      const blockHash = matchObj?.blockHash ?? rawBlock?.header?.hash;
      let txIndexInBlock = getTxIndexInBlock(rawBlock, txId);
      if (txIndexInBlock < 0 && blockHash) txIndexInBlock = 0;

      if (blueScore == null || blockHash == null) {
        log.warn("ConfirmationWatcher: match missing blueScore or blockHash");
        return;
      }

      clearTimeout(timeout);
      unsubscribe();
      resolve({
        blockHash: String(blockHash),
        blueScore: Number(blueScore),
        txIndexInBlock,
      });
    };

    const unsubscribe = scanner.subscribeMatch(onMatch);
  });
}
