/* MediaServo Field C ABI (推流面) — 车端 SDK 消费。
 *
 * MAJOR = C ABI 版本 (D241)：within MAJOR 只加法，二进制兼容。
 * 本头文件手工维护（稳定导出面，等价 cbindgen 输出纪律）。
 * 共享 C 类型（mediaservo_err_t/mediaservo_frame_meta_t/mediaservo_frame_t）见 common.h。
 *
 * 用法:
 *   mediaservo_push_config_t cfg = MEDIASERVO_PUSH_CONFIG_DEFAULT;
 *   cfg.url = "ws://host:9800/ws"; cfg.psk = "..."; cfg.room = "...";
 *   mediaservo_field_push_t* s = NULL;
 *   if (mediaservo_field_push_connect(&cfg, &s) != MEDIASERVO_OK) { 读 mediaservo_field_last_error }
 *   char track[64];
 *   mediaservo_field_push_publish_video(s, track, sizeof(track));
 *   mediaservo_field_push_start_video_frames(s);
 *   ...
 *   mediaservo_field_push_close(s);
 *
 * 生命周期契约：handle 单线程属主；close 后任何 API 调用为 UB（close 幂等）。
 */
#ifndef MEDIASERVO_FIELD_H
#define MEDIASERVO_FIELD_H

#include <stddef.h>
#include <stdint.h>

#include <mediaservo/common.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── 错误码（0 = ok, <0 = error）── */
#define MEDIASERVO_FIELD_ERR_INVALID_ARG  (-1)
#define MEDIASERVO_FIELD_ERR_CONNECT      (-2)
#define MEDIASERVO_FIELD_ERR_PUBLISH      (-3)
#define MEDIASERVO_FIELD_ERR_STATE        (-4)
#define MEDIASERVO_FIELD_ERR_INTERNAL     (-5)
/* 历史别名（additive-only 保留，新代码用 MEDIASERVO_FIELD_ERR_*） */
#define MEDIASERVO_ERR_INVALID_ARG        (-1)
#define MEDIASERVO_ERR_CONNECT            (-2)
#define MEDIASERVO_ERR_PUBLISH            (-3)
#define MEDIASERVO_ERR_STATE              (-4)
#define MEDIASERVO_ERR_INTERNAL           (-5)

/* ── 推流配置 ──
 * struct_size 首字段（R3）：调用方必须填 sizeof(mediaservo_push_config_t)，
 * 库校验 >= sizeof(已知结构)、超长忽略 —— 结构演进不破坏二进制兼容。
 */
typedef struct mediaservo_push_config_t {
    size_t struct_size;           /* sizeof(mediaservo_push_config_t) */
    const char* url;              /* WS 信令地址，如 "ws://host:9800/ws" */
    const char* psk;              /* PSK 认证密钥 */
    const char* room;             /* 房间 ID */
    uint32_t width;               /* 视频宽 (默认 1280) */
    uint32_t height;              /* 视频高 (默认 720) */
    uint32_t framerate;           /* 帧率 (默认 30) */
    uint32_t bitrate_kbps;        /* 编码码率 kbps (默认 2000) */
    uint64_t keyframe_interval;   /* 关键帧间隔秒 (默认 2) */
} mediaservo_push_config_t;

#define MEDIASERVO_PUSH_CONFIG_DEFAULT { sizeof(mediaservo_push_config_t), NULL, NULL, NULL, 1280, 720, 30, 2000, 2 }

/* ── opaque handle ── */
typedef struct mediaservo_field_push_t mediaservo_field_push_t;

/* ── 推流会话 API ── */

/* 连接信令 + 创建会话（阻塞）。成功: *out 指向新 handle（调用方 close）。 */
mediaservo_err_t mediaservo_field_push_connect(const mediaservo_push_config_t* cfg, mediaservo_field_push_t** out);

/* 发布视频轨（阻塞协商）。track id 写入 out_track（至少 64 字节）。 */
mediaservo_err_t mediaservo_field_push_publish_video(mediaservo_field_push_t* s, char* out_track, size_t out_track_len);

/* 启动视频帧生成（Squares + 时间戳水印）。 */
mediaservo_err_t mediaservo_field_push_start_video_frames(mediaservo_field_push_t* s);

/* 停止视频帧生成（幂等）。 */
void mediaservo_field_push_stop_video_frames(mediaservo_field_push_t* s);

/* 关闭会话并释放 handle（幂等）。 */
mediaservo_err_t mediaservo_field_push_close(mediaservo_field_push_t* s);

/* ── 通用 ── */

/* 最近一次错误详情（线程安全）。 */
mediaservo_err_t mediaservo_field_last_error(char* buf, size_t len);

/* 最近一次错误详情（deprecated 别名，additive-only 保留）。 */
mediaservo_err_t mediaservo_last_error(char* buf, size_t len);

/* SDK 版本 (MAJOR.MINOR.PATCH)。 */
mediaservo_err_t mediaservo_field_version(char* buf, size_t len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MEDIASERVO_FIELD_H */
