/**
 * audioChunker.js - Split encoded Opus into chunks that fit Kaspa payload
 * Single responsibility: chunking only; no send.
 */

/**
 * Split encoded Opus bytes into chunks each <= maxChunkBytes.
 * @param {Uint8Array} opusBytes - Full encoded Opus (or Ogg Opus) data
 * @param {number} maxChunkBytes - Max size per chunk (payload limit minus metadata)
 * @returns {Uint8Array[]} Array of chunks
 */
export function chunkOpusBytes(opusBytes, maxChunkBytes) {
  if (!opusBytes?.length || maxChunkBytes <= 0) return [];
  const chunks = [];
  let offset = 0;
  while (offset < opusBytes.length) {
    const end = Math.min(offset + maxChunkBytes, opusBytes.length);
    chunks.push(opusBytes.slice(offset, end));
    offset = end;
  }
  return chunks;
}
