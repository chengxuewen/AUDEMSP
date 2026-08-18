// node:test — 错误路径 + 本地帧验证（无需 server；真 server 场景见 examples/）。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PushSession, SignalSession, CameraSource } from '../lib/index.mjs';

test('PushSession.connect rejects on empty config', async () => {
  await assert.rejects(PushSession.connect({ url: '', psk: '', room: '' }));
});

test('SignalSession.connect rejects on unknown role', async () => {
  await assert.rejects(SignalSession.connect({ url: 'ws://x', psk: 'p', room: 'r', role: 'bogus' }));
});

test('CameraSource frames arrive via onFrame (I420)', async () => {
  const cam = await CameraSource.open('stub:test-camera', { width: 320, height: 180, framerate: 10 });
  await cam.start();
  let frame = null;
  await cam.onFrame((f) => { if (!frame) frame = f; });
  await new Promise((r) => setTimeout(r, 800));
  await cam.stop();
  await cam.close();
  assert.ok(frame, 'frame received');
  assert.equal(frame.width, 320);
  assert.equal(frame.height, 180);
  assert.equal(frame.data.length, 320 * 180 * 3 / 2, 'I420 size');
});

test('CameraSource.open rejects unknown device', async () => {
  await assert.rejects(CameraSource.open('nope', {}));
});
