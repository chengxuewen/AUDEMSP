/* MediaServo link C ABI (信令 + 帧总线) — 设备侧 SDK 消费。
 *
 * MAJOR = C ABI 版本 (D241)：within MAJOR 只加法，二进制兼容。
 * 本头文件手工维护（稳定导出面，等价 cbindgen 输出纪律）。
 * 共享 C 类型（ms_err_t/ms_frame_meta_t/ms_frame_t）见 mediaservo_common.h。
 *
 * 用法（信令）:
 *   ms_link_signal_config_t cfg = MS_LINK_SIGNAL_CONFIG_DEFAULT;
 *   cfg.url = "ws://host:9800/ws"; cfg.psk = "..."; cfg.room = "...";
 *   ms_link_signal_t* s = NULL;
 *   if (ms_link_signal_connect(&cfg, &s) != MS_OK) { 读 ms_link_last_error }
 *   ms_link_signal_on_event(s, on_event, NULL);   -- 任意时刻注册；重复注册替换
 *   ms_link_signal_send(s, "{\"type\":\"frame\",...}", len);
 *   ...
 *   ms_link_signal_close(s);
 *
 * 生命周期契约（R2）：
 * - handle 单线程属主；close 后任何 API 调用为 UB（close(NULL) 幂等返回 OK）。
 * - close = 置 closed 标志 → 释放会话（唤醒事件泵）→ join 泵线程 → 释放内存。
 * - 事件回调仅在一个内部泵线程触发，回调调用期间不持任何锁；回调必须快速
 *   返回；回调内禁止调用任何 ms_link_signal_* API（含 close）——未定义行为。
 * - 事件 JSON 字符串仅在回调内有效（需保留请拷贝）。
 * - on_event 注册前发生的事件可能丢失（broadcast 只保留订阅后消息）；首次
 *   注册时泵合成补发 {"type":"connected"} 事件。
 *
 * 事件 JSON（opaque v1）:
 *   {"type":"connected","room_id":...}
 *   {"type":"message","message":{...SignalingMessage}}
 *   {"type":"disconnected","reason":...}
 *   {"type":"error","error":...}
 */
#ifndef MEDIASERVO_LINK_H
#define MEDIASERVO_LINK_H

#include <stddef.h>
#include <stdint.h>

#include "mediaservo_common.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ── 错误码（0 = ok, <0 = error）── */
#define MS_LINK_ERR_INVALID_ARG  (-1)
#define MS_LINK_ERR_CONNECT      (-2)
#define MS_LINK_ERR_SEND         (-3)
#define MS_LINK_ERR_BUS          (-4)
#define MS_LINK_ERR_STATE        (-5)
#define MS_LINK_ERR_INTERNAL     (-6)
#define MS_LINK_ERR_CLOSED       (-7)

/* ── 信令配置 ──
 * struct_size 首字段（R3）：调用方必须填 sizeof(ms_link_signal_config_t)，
 * 库校验 >= sizeof(已知结构)、超长忽略 —— 结构演进不破坏二进制兼容。
 */
typedef struct ms_link_signal_config_t {
    size_t struct_size;      /* sizeof(ms_link_signal_config_t) */
    const char* url;         /* WS 信令地址，如 "ws://host:9800/ws" */
    const char* psk;         /* PSK 认证密钥 */
    const char* room;        /* 房间 ID */
    const char* role;        /* "Host"/"Pusher"→Host, "Client"/"Puller"→Remote; NULL=Host */
} ms_link_signal_config_t;

#define MS_LINK_SIGNAL_CONFIG_DEFAULT { sizeof(ms_link_signal_config_t), NULL, NULL, NULL, NULL }

/* ── opaque handle ── */
typedef struct ms_link_signal_t ms_link_signal_t;
typedef struct ms_link_bus_t ms_link_bus_t;
typedef struct ms_link_stream_t ms_link_stream_t;

/* ── 信令事件回调（仅在一个内部泵线程触发；事件串仅在回调内有效）── */
typedef void (*ms_link_event_cb)(ms_link_signal_t* s, const char* event_json, void* user);

/* ── 信令会话 API ── */

/* 连接信令 + 创建会话（阻塞）。成功: *out 指向新 handle（调用方 close）。 */
ms_err_t ms_link_signal_connect(const ms_link_signal_config_t* cfg, ms_link_signal_t** out);

/* 发送一条信令消息（JSON + 字节长度；SignalingMessage type 标签 snake_case）。 */
ms_err_t ms_link_signal_send(ms_link_signal_t* s, const char* msg_json, size_t len);

/* 注册事件回调（connect 后任意时刻；重复注册替换；首次注册启动事件泵）。 */
void ms_link_signal_on_event(ms_link_signal_t* s, ms_link_event_cb cb, void* user);

/* 关闭会话并释放 handle（幂等；join 事件泵后才释放内存）。 */
ms_err_t ms_link_signal_close(ms_link_signal_t* s);

/* ── 帧总线 API ── */

/* 附加帧总线（验签 + ACL + iceoryx2 节点，阻塞）。endpoint 为 Phase 1 预留
 * （可传空串）；token_pem/vk_pem 为 Ed25519 能力令牌/验证密钥 PEM 字符串。 */
ms_err_t ms_link_bus_attach(const char* endpoint, const char* token_pem,
                            const char* vk_pem, ms_link_bus_t** out);

/* 发布一帧（ACL 检查 + SHM loan + send，阻塞）。meta 为 36B 字段袋
 * （逐字段读取）；payload 可为 NULL 当且仅当 len == 0（纯元数据帧）。 */
ms_err_t ms_link_bus_publish(ms_link_bus_t* b, const char* topic,
                             const uint8_t* payload, size_t len,
                             const ms_frame_meta_t* meta);

/* 订阅 topic，创建帧流 handle（阻塞）。成功: *out 指向新 handle（调用方
 * ms_link_stream_close）。 */
ms_err_t ms_link_bus_subscribe(ms_link_bus_t* b, const char* topic, ms_link_stream_t** out);

/* 阻塞取帧：元数据拷入 out_meta，载荷拷入 out_data（最多 cap 字节），
 * *out_len = 实际拷贝字节数（帧大于 cap 时截断——大帧请给足 cap）。
 * 关停（stream_close / bus_close）→ MS_LINK_ERR_CLOSED。 */
ms_err_t ms_link_bus_recv(ms_link_stream_t* st, ms_frame_meta_t* out_meta,
                          uint8_t* out_data, size_t cap, size_t* out_len);

/* 关闭帧流 handle（幂等；唤醒阻塞中的 recv 使其返回 CLOSED）。 */
ms_err_t ms_link_stream_close(ms_link_stream_t* st);

/* 关闭帧总线 handle（幂等；shutdown 全部流，recv 返回 CLOSED）。 */
ms_err_t ms_link_bus_close(ms_link_bus_t* b);

/* ── 通用 ── */

/* 最近一次错误详情（线程安全）。 */
ms_err_t ms_link_last_error(char* buf, size_t len);

/* SDK 版本 (MAJOR.MINOR.PATCH)。 */
ms_err_t ms_link_version(char* buf, size_t len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MEDIASERVO_LINK_H */
