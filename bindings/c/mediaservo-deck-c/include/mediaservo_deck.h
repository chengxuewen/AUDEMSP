/* MediaServo Deck C ABI (采集/录制/回放面) — 本地监控/NVR 场景 C 消费。
 *
 * MAJOR = C ABI 版本 (D241)：within MAJOR 只加法，二进制兼容。
 * 本头文件手工维护（稳定导出面，等价 cbindgen 输出纪律）。
 * 共享 C 类型（ms_err_t/ms_frame_t）见 mediaservo_common.h。
 *
 * 用法:
 *   ms_deck_camera_t* cam = NULL;
 *   ms_deck_capture_options_t copts = MS_DECK_CAPTURE_OPTIONS_DEFAULT; /* 1280x720@30 */
 *   ms_deck_camera_open("stub:test-camera", &copts, &cam);
 *   ms_deck_camera_start(cam);
 *   ms_deck_camera_frames_cb(cam, on_frame, NULL);  /* 泵线程逐帧回调 */
 *   ...
 *   ms_deck_camera_stop(cam);
 *   ms_deck_camera_close(cam);
 *
 * 生命周期契约：
 *   - handle 单线程属主；close 后指针失效，任何 API 调用为 UB
 *     （重复 close 同一指针同样为 UB —— C 惯例：close 后置 NULL）。
 *   - ms_frame_t 的 data_* 指针仅在回调内有效 —— 需要保留必须拷贝。
 *   - 帧回调仅在泵线程触发；回调内禁止调用任何 ms_deck_* API（含 close）。
 *   - ms_deck_recorder_record(rec, cam) 要求 camera 已 start 且活到录制结束；
 *     关闭顺序必须 recorder_stop/close 先于 camera_stop/close。
 */
#ifndef MEDIASERVO_DECK_H
#define MEDIASERVO_DECK_H

#include <stddef.h>
#include <stdint.h>

#include "mediaservo_common.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ── 错误码（0 = ok, <0 = error）── */
#define MS_DECK_ERR_INVALID_ARG  (-1)
#define MS_DECK_ERR_DEVICE       (-2)
#define MS_DECK_ERR_RECORDER     (-3)
#define MS_DECK_ERR_PLAYER       (-4)
#define MS_DECK_ERR_STATE        (-5)
#define MS_DECK_ERR_INTERNAL     (-6)

/* ── 采集选项 ──
 * struct_size 首字段（R3）：调用方必须填 sizeof(ms_deck_capture_options_t)，
 * 库校验 >= sizeof(已知结构)、超长忽略 —— 结构演进不破坏二进制兼容。
 * width/height/framerate 全 0 = 默认 1280x720@30。
 */
typedef struct ms_deck_capture_options_t {
    size_t struct_size;           /* sizeof(ms_deck_capture_options_t) */
    uint32_t width;               /* 视频宽 (0 = 默认 1280) */
    uint32_t height;              /* 视频高 (0 = 默认 720) */
    uint32_t framerate;           /* 帧率 (0 = 默认 30) */
} ms_deck_capture_options_t;

#define MS_DECK_CAPTURE_OPTIONS_DEFAULT { sizeof(ms_deck_capture_options_t), 0, 0, 0 }

/* ── opaque handle ── */
typedef struct ms_deck_camera_t ms_deck_camera_t;
typedef struct ms_deck_recorder_t ms_deck_recorder_t;
typedef struct ms_deck_player_t ms_deck_player_t;

/* ── 帧回调（泵线程触发；frame 指针仅回调内有效）── */
typedef void (*ms_deck_frame_cb)(const ms_frame_t* frame, void* user);

/* ── 设备枚举 ── */

/* 双调用模式: 第一次 out_ids=NULL 返回所需长度（不含 NUL；错误为负值），
 * 第二次填缓冲（截断时同样返回所需长度，snprintf 约定）。
 * kind: 0=Camera 1=Audio 2=Screen；多设备用 '\n' 分隔。
 * out_len 非空时两次调用均写回所需长度。 */
ms_err_t ms_deck_devices_enumerate(int kind, char* out_ids, size_t cap, size_t* out_len);

/* ── camera（采集）── */

/* 打开相机（仅本地初始化）。dev_id 必须存在于枚举结果。 */
ms_err_t ms_deck_camera_open(const char* dev_id,
                             const ms_deck_capture_options_t* opts,
                             ms_deck_camera_t** out);

/* 开始产帧（用 open 时的 opts；只允许一次，重复调用 → STATE）。 */
ms_err_t ms_deck_camera_start(ms_deck_camera_t* c);

/* 注册帧回调（泵线程逐帧触发；重复调用替换旧回调）。 */
ms_err_t ms_deck_camera_frames_cb(ms_deck_camera_t* c, ms_deck_frame_cb cb, void* user);

/* 停止产帧（幂等）。 */
ms_err_t ms_deck_camera_stop(ms_deck_camera_t* c);

/* 关闭相机并释放 handle（幂等；帧回调期间调用为 UB）。 */
ms_err_t ms_deck_camera_close(ms_deck_camera_t* c);

/* ── recorder（录制）── */

/* 创建录制器（默认 h264/mp4；父目录必须已存在）。 */
ms_err_t ms_deck_recorder_new(const char* path, ms_deck_recorder_t** out);

/* 桥接录制: camera 帧泵 → recorder。camera 必须已 start 且活到录制结束。 */
ms_err_t ms_deck_recorder_record(ms_deck_recorder_t* r, ms_deck_camera_t* c);

/* 请求停止录制（幂等；flush + trailer 收尾在 close 时完成）。 */
ms_err_t ms_deck_recorder_stop(ms_deck_recorder_t* r);

/* 关闭录制器并释放 handle（幂等；join 录制任务完成 flush）。 */
ms_err_t ms_deck_recorder_close(ms_deck_recorder_t* r);

/* ── player（回放）── */

/* 打开媒体文件（demux + 解码器就绪）。 */
ms_err_t ms_deck_player_open(const char* path, ms_deck_player_t** out);

/* 逐帧解码回调泵（EOF 或 close 后退出；只允许一次）。 */
ms_err_t ms_deck_player_frames_cb(ms_deck_player_t* p, ms_deck_frame_cb cb, void* user);

/* 关闭回放器并释放 handle（幂等）。 */
ms_err_t ms_deck_player_close(ms_deck_player_t* p);

/* ── 通用 ── */

/* 最近一次错误详情（线程安全）。 */
ms_err_t ms_deck_last_error(char* buf, size_t len);

/* SDK 版本 (MAJOR.MINOR.PATCH)。 */
ms_err_t ms_deck_version(char* buf, size_t len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MEDIASERVO_DECK_H */
