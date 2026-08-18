/* MediaServo 共享 C 类型 — 所有 SDK 头文件的公共基座。
 *
 * MAJOR = C ABI 版本 (D241)：within MAJOR 只加法，二进制兼容。
 * 本头文件手工维护（稳定导出面，等价 cbindgen 输出纪律）。
 */
#ifndef MEDIASERVO_COMMON_H
#define MEDIASERVO_COMMON_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── 错误码 ──
 * 每 SDK 头文件按 MEDIASERVO_<SDK>_ERR_* 前缀展开值空间（防多 header 同 include 宏冲突）：
 *   field: MEDIASERVO_FIELD_ERR_*（保留历史 MEDIASERVO_ERR_* 别名）
 *   link:  MEDIASERVO_LINK_ERR_*
 *   deck:  MEDIASERVO_DECK_ERR_*
 * 0 = ok，<0 = 错误码。
 */
typedef int mediaservo_err_t;
#define MEDIASERVO_OK 0

/* ── 帧元数据（link 帧总线线格式，定长 36B LE，D243）────────
 * 布局与 Rust FrameMeta::encode 逐字节一致：
 *   seq(8) + width(4) + height(4) + format(1) + version(1)
 *   + is_keyframe(1) + reserved(1) + ts_mono_ns(8) + ts_epoch_ns(8)
 *
 * 注意：#pragma pack(1) 强制 36B —— 自然对齐会因尾部 u64 对齐产生
 * 4B 填充（40B），与线格式不符。C 结构只是字段袋：Rust 侧逐字段
 * 读取构造 FrameMeta，禁止整块 reinterpret（填充/字节序风险）。
 */
#pragma pack(push, 1)
typedef struct mediaservo_frame_meta_t {
    uint64_t seq;
    uint32_t width;
    uint32_t height;
    uint8_t  format;        /* 0=未知, 1=I420, 2=NV12, 3=RGBA */
    uint8_t  version;       /* 元数据版本（演进用，D243） */
    uint8_t  is_keyframe;   /* 0/1 */
    uint8_t  reserved;      /* 必须填 0 */
    uint64_t ts_mono_ns;    /* 单调时钟 ns */
    uint64_t ts_epoch_ns;   /* 墙上时钟 ns */
} mediaservo_frame_meta_t;
#pragma pack(pop)

/* 编译期尺寸断言（C/C++ 通用，避免 _Static_assert 的 C11 依赖） */
typedef char mediaservo_frame_meta_size_check[(sizeof(mediaservo_frame_meta_t) == 36) ? 1 : -1];

/* ── 内存帧描述（deck 采集/回放回调，I420 三平面）────────
 * data_* 指针仅在回调内有效 —— 需要保留必须拷贝（文档契约）。
 */
typedef struct mediaservo_frame_t {
    uint32_t width;
    uint32_t height;
    uint64_t pts_us;        /* 演示时间戳 µs */
    uint32_t stride_y;
    uint32_t stride_u;
    uint32_t stride_v;
    const uint8_t* data_y;
    const uint8_t* data_u;
    const uint8_t* data_v;
} mediaservo_frame_t;

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MEDIASERVO_COMMON_H */
