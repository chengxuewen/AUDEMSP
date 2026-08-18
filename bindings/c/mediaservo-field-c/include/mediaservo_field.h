/* MediaServo Field C ABI (推流面) — 车端 SDK 消费。
 *
 * MAJOR = C ABI 版本 (D241)：within MAJOR 只加法，二进制兼容。
 * 本头文件手工维护（稳定导出面，等价 cbindgen 输出纪律）。
 *
 * 用法:
 *   ms_push_config_t cfg = MS_PUSH_CONFIG_DEFAULT;
 *   cfg.url = "ws://host:9800/ws"; cfg.psk = "..."; cfg.room = "...";
 *   ms_field_push_t* s = NULL;
 *   if (ms_field_push_connect(&cfg, &s) != MS_OK) { /* 读 ms_last_error */ }
 *   char track[64];
 *   ms_field_push_publish_video(s, track, sizeof(track));
 *   ms_field_push_start_video_frames(s);
 *   ...
 *   ms_field_push_close(s);
 */
#ifndef MEDIASERVO_FIELD_H
#define MEDIASERVO_FIELD_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── 错误码 ── */
#define MS_OK                0
#define MS_ERR_INVALID_ARG  (-1)
#define MS_ERR_CONNECT      (-2)
#define MS_ERR_PUBLISH      (-3)
#define MS_ERR_STATE        (-4)
#define MS_ERR_INTERNAL     (-5)

typedef int ms_err_t;

/* ── 推流配置 ── */
typedef struct ms_push_config_t {
    const char* url;              /* WS 信令地址，如 "ws://host:9800/ws" */
    const char* psk;              /* PSK 认证密钥 */
    const char* room;             /* 房间 ID */
    uint32_t width;               /* 视频宽 (默认 1280) */
    uint32_t height;              /* 视频高 (默认 720) */
    uint32_t framerate;           /* 帧率 (默认 30) */
    uint32_t bitrate_kbps;        /* 编码码率 kbps (默认 2000) */
    uint64_t keyframe_interval;   /* 关键帧间隔秒 (默认 2) */
} ms_push_config_t;

#define MS_PUSH_CONFIG_DEFAULT { NULL, NULL, NULL, 1280, 720, 30, 2000, 2 }

/* ── opaque handle ── */
typedef struct ms_field_push_t ms_field_push_t;

/* ── 推流会话 API ── */

/* 连接信令 + 创建会话（阻塞）。成功: *out 指向新 handle（调用方 close）。 */
ms_err_t ms_field_push_connect(const ms_push_config_t* cfg, ms_field_push_t** out);

/* 发布视频轨（阻塞协商）。track id 写入 out_track（至少 64 字节）。 */
ms_err_t ms_field_push_publish_video(ms_field_push_t* s, char* out_track, size_t out_track_len);

/* 启动视频帧生成（Squares + 时间戳水印）。 */
ms_err_t ms_field_push_start_video_frames(ms_field_push_t* s);

/* 停止视频帧生成（幂等）。 */
void ms_field_push_stop_video_frames(ms_field_push_t* s);

/* 关闭会话并释放 handle。 */
ms_err_t ms_field_push_close(ms_field_push_t* s);

/* ── 通用 ── */

/* 最近一次错误详情（线程安全）。 */
ms_err_t ms_last_error(char* buf, size_t len);

/* SDK 版本 (MAJOR.MINOR.PATCH)。 */
ms_err_t ms_field_version(char* buf, size_t len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MEDIASERVO_FIELD_H */
