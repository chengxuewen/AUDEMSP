// MediaServo Node.js 绑定 — 薄 TS 层（livekit-rtc 式包装 over napi .node）。
// 使用: import { PushSession, SignalSession, CameraSource } from 'mediaservo';
// 加载: 同目录 mediaservo.node（构建: cargo build -p mediaservo-node + cp .so → .node）

import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const binding = require('../mediaservo.node');

/**
 * 推流配置。
 * @typedef {Object} PushConfig
 * @property {string} url - WS 信令地址（如 ws://host:9800/ws）
 * @property {string} psk - PSK 认证密钥
 * @property {string} room - 房间 ID
 * @property {number} [width] - 视频宽（默认 1280）
 * @property {number} [height] - 视频高（默认 720）
 * @property {number} [framerate] - 帧率（默认 30）
 * @property {number} [bitrateKbps] - 码率 kbps（默认 2000）
 */
export class PushSession {
  /** @private */
  constructor(h) { this._h = h; }

  /** 连接信令 + 创建会话（async）。 */
  static async connect(cfg) {
    return new PushSession(await binding.JsPushSession.connect(cfg));
  }
  /** 发布视频轨（SFU 协商）。@returns {Promise<string>} track id */
  publishVideo() { return this._h.publishVideo(); }
  startVideoFrames() { return this._h.startVideoFrames(); }
  stopVideoFrames() { return this._h.stopVideoFrames(); }
  close() { return this._h.close(); }
}

/**
 * 信令配置。role: "Host"|"Pusher"|"Client"|"Puller"（默认 Host）。
 * @typedef {Object} SignalConfig
 * @property {string} url
 * @property {string} psk
 * @property {string} room
 * @property {string} [role]
 */
export class SignalSession {
  /** @private */
  constructor(h) { this._h = h; }

  static async connect(cfg) {
    return new SignalSession(await binding.JsSignalSession.connect(cfg));
  }
  /** 发送信令消息（SignalingMessage JSON）。 */
  send(json) { return this._h.send(json); }
  /** 订阅事件（JSON 字符串回调；事件对象 {type, ...}）。 */
  onEvent(cb) { return this._h.onEvent((ev) => cb(JSON.parse(ev))); }
  close() { return this._h.close(); }
}

/**
 * 采集选项。
 * @typedef {Object} CaptureOptions
 * @property {number} [width]
 * @property {number} [height]
 * @property {number} [framerate]
 */
export class CameraSource {
  /** @private */
  constructor(h) { this._h = h; }

  /** 打开相机（当前 stub 设备 "stub:test-camera"）。 */
  static async open(devId, opts = {}) {
    return new CameraSource(await binding.JsCameraSource.open(devId, opts));
  }
  start() { return this._h.start(); }
  /**
   * 订阅帧回调。napi 元组 → JS 数组 [meta, data] → 解构为帧对象。
   * @param {(frame: {width:number,height:number,ptsUs:number,keyframe:boolean,data:Uint8Array}) => void} cb
   */
  onFrame(cb) {
    return this._h.onFrame(([meta, data]) => cb({ ...JSON.parse(meta), data }));
  }
  stop() { return this._h.stop(); }
  close() { return this._h.close(); }
}

export default { PushSession, SignalSession, CameraSource };
