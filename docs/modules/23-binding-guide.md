# 绑定矩阵使用指引（link / deck / field × C / C++ / Python）

> **定位**: MediaServo 三 SDK（link 设备侧 IPC、deck 媒体数据面、field 组合推流面）的
> 多语言消费层。**C ABI 为契约基座**，C++/Python 为其上的薄包装（D227）。
>
> **关联**: [20-sdk-api-contract.md](../modules/20-sdk-api-contract.md) §7、[22-field-guide.md](../modules/22-field-guide.md)、
> D227（绑定族 c/cxx/py）、D240（单动态库）、D241（soname + ABI 稳定）、D247（mediaservo_ 前缀）、D248（头文件策略）

## 布局

```
bindings/
├── c/
│   ├── include/mediaservo/common.h        # 共享 C 类型（err_t/frame_meta_t 36B/frame_t）
│   ├── mediaservo-field-c/                # cdylib → libmediaservo_field.so
│   │   └── include/mediaservo/field.h     #   头文件（手工维护，D248）
│   ├── mediaservo-link-c/                 # → libmediaservo_link.so（signal + bus）
│   │   └── include/mediaservo/link.h
│   └── mediaservo-deck-c/                 # → libmediaservo_deck.so（camera/recorder/player）
│       └── include/mediaservo/deck.h
├── cxx/mediaservo-{field,link,deck}-cxx/  # header-only RAII（namespace mediaservo::{field,link,deck}）
└── python/mediaservo/                     # ctypes 包（非 cargo member，D228）
    └── mediaservo/{_ffi,field,link,deck}.py
```

## 构建与测试（pixi tasks）

```bash
pixi run build-c          # 三 cdylib + dev .so.<MAJOR> symlink（D241 DT_NEEDED）
pixi run test-cxx         # C++ 三 SDK 测试（g++ 编译 + 断言）
pixi run test-py          # Python unittest（需 MEDIASERVO_LIB_DIR 或 env 继承）
pixi run parity-bindings  # 跨语言一致性（version/错误路径三端断言）
pixi run abi-drift        # ABI 漂移门禁（header 声明 ↔ .so 导出对照，D248）
```

## 各语言集成

### C（契约基座）

```c
#include <mediaservo/field.h>          /* 或 link.h / deck.h */

mediaservo_push_config_t cfg = MEDIASERVO_PUSH_CONFIG_DEFAULT;  /* struct_size 自动 */
cfg.url = "ws://host:9800/ws"; cfg.psk = "..."; cfg.room = "room";
mediaservo_field_push_t* s = NULL;
if (mediaservo_field_push_connect(&cfg, &s) != MEDIASERVO_OK) {
    char err[256]; mediaservo_field_last_error(err, sizeof(err));
}
```

编译: `gcc app.c -I bindings/c/mediaservo-field-c/include -I bindings/c/include \
       -L target/debug -lmediaservo_field`（运行 `LD_LIBRARY_PATH=target/debug`）

### C++（header-only RAII）

```cpp
#include <mediaservo/field.hpp>        /* 或 link.hpp / deck.hpp */
using mediaservo::field::PushConfig;
using mediaservo::field::PushSession;

auto s = PushSession::connect(PushConfig{"ws://host:9800/ws", "psk", "room"});
if (!s) { /* s.error().code / s.error().message */ }
s.value().start_video_frames();        /* RAII: 析构自动 close */
```

编译: `g++ -std=c++17 app.cpp -I bindings/cxx/mediaservo-field-cxx/include \
       -I bindings/c/mediaservo-field-c/include -I bindings/c/include \
       -L target/debug -lmediaservo_field`

### Python（ctypes）

```python
from mediaservo.field import PushConfig, PushSession
s = PushSession.connect(PushConfig(url="ws://host:9800/ws", psk="...", room="room"))
s.publish_video()
```

运行: `export MEDIASERVO_LIB_DIR=$PWD/target/debug && python3 app.py`
（或 `LD_LIBRARY_PATH=target/debug`；三 SDK 子模块 `mediaservo.{field,link,deck}`）

## 契约要点（跨语言一致）

| 项 | 规则 |
|---|---|
| 前缀 | 符号/类型/宏 `mediaservo_*`（D247）；C++ 命名空间 `mediaservo::<sdk>`；Python 模块同名 |
| soname | `libmediaservo_<sdk>.so.<MAJOR>`（D241，MAJOR = C ABI 版本）|
| struct_size | 所有跨 FFI 配置结构体首字段 `size_t struct_size`（R3，调用方填 sizeof）|
| 生命周期 | handle 单线程属主；close 后任何 API 调用为 UB；close 幂等 |
| 回调 | 仅在泵线程触发；回调内禁止调 close；指针/事件字符串仅回调内有效（需保留请拷贝）|
| 错误 | `mediaservo_<sdk>_last_error(buf, len)` 线程安全；0 = ok，<0 = 错误码 |
| 版本 | `mediaservo_<sdk>_version(buf, len)` → MAJOR.MINOR.PATCH |
| 兼容 | within MAJOR 只加法（D241）；头文件改动 = ABI 变更，须过 `abi-drift` |

## 已验证能力（2026-08-18）

| 语言 | 验证 |
|---|---|
| C | field/link/deck 三端 live e2e（真实 server 收帧 / 事件泵 / 91 帧闭环录制）|
| C++ | 三 SDK 测试（version/错误路径/move 语义/Result 误用）+ parity |
| Python | 22 tests + field push live e2e（connected/published/frames）|
| 全矩阵 | `parity-bindings` 三端一致 + `abi-drift` 声明=导出（7/12/15）|

## 已知限制

- **PullSession 收帧挂起**（见 22-field-guide 已知限制）：消费方是 client，未在绑定层暴露
- **deck Player**：C 面仅 open/frames/close（seek/set_rate YAGNI 押后，N2 标注）
- **Python**：ctypes 首版（D227 两步走）；pyo3 加速后端待触发条件（帧路径 >10% 预算等）
- **Windows/macOS**：soname 仅 Linux（R8 门控）；macOS 用默认 dylib 命名；Python `_lib_filename` 已按平台分派
