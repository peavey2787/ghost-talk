/**
 * queuePosition.js - Queue position type and ordering for voice (blueScore + txIndexInBlock)
 * Single responsibility: compare queue positions; no I/O.
 */

/**
 * Compare two queue positions for ordering.
 * @param {{ blueScore: number, txIndexInBlock: number }} a
 * @param {{ blueScore: number, txIndexInBlock: number }} b
 * @returns {number} -1 if a < b, 0 if equal, 1 if a > b
 */
export function compareQueuePosition(a, b) {
  if (a.blueScore !== b.blueScore) return a.blueScore < b.blueScore ? -1 : 1;
  if (a.txIndexInBlock !== b.txIndexInBlock) {
    return a.txIndexInBlock < b.txIndexInBlock ? -1 : 1;
  }
  return 0;
}

/**
 * Unique key for a queue position (for Map keys / ordering).
 * @param {{ blueScore: number, txIndexInBlock: number }} pos
 * @returns {string}
 */
export function queuePositionKey(pos) {
  return `${pos.blueScore}:${pos.txIndexInBlock}`;
}
