/**
 * leaseManager.js - Configurable airtime lease; start on RTT confirm, emit expired
 * Single responsibility: timer only; no Kaspa.
 */

import { EventEmitter } from "../../core/eventEmitter.js";

export const LeaseEvent = Object.freeze({
  EXPIRED: "expired",
});

/**
 * Manages a single "lease" (e.g. 20s) after RTT confirmation.
 */
export class LeaseManager extends EventEmitter {
  constructor() {
    super();
    this._timer = null;
    this._leaseSeconds = 20;
  }

  /**
   * @param {number} [seconds] - Lease duration (default from config)
   */
  setLeaseSeconds(seconds) {
    if (seconds != null && Number.isFinite(seconds) && seconds > 0) {
      this._leaseSeconds = seconds;
    }
  }

  /**
   * Start lease countdown; cancels any existing lease.
   */
  start() {
    this.cancel();
    this._timer = setTimeout(() => {
      this._timer = null;
      this.emit(LeaseEvent.EXPIRED);
    }, this._leaseSeconds * 1000);
  }

  /**
   * Cancel current lease.
   */
  cancel() {
    if (this._timer) {
      clearTimeout(this._timer);
      this._timer = null;
    }
  }

  get isActive() {
    return this._timer != null;
  }
}
