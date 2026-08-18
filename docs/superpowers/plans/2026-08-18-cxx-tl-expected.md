# 计划: C++ 绑定完全迁移 tl::expected — C++11 起步 + 原生 API（2026-08-18）

> **来源**: hyperplan 对抗规划（4 视角: arch/integration/cost/creative + lead 交叉攻击 + C++11 实证裁决 + plan agent 两轮修订）
> **状态**: 待用户确认执行 | **关联**: 23-binding-guide、D248、契约 §7

## 裁决摘要

放弃兼容层（Result 包装保持 ok()/operator bool），**完全迁移 tl::expected 原生 API**：
`Result<T>` = alias `tl::expected<T, Error>`（非包装类），工厂 success()/failure() 删除，误用抛
`tl::bad_expected_access<Error>`（std::exception 子类）。理由: 零真实消费者 → 未来消费者学标准 API
价值更高；半迁移比不迁移糟；Jetson 编译器升级后一行 swap 到 std::expected；D241 只锁 C ABI，
C++ header API 无 ABI 承诺，source-breaking 可接受。

## 实证基线（已跑通, `-std=c++11 -Wall -Wextra` 零错零警）

1. tl::expected 1.2.0 C++11 全绿；`expected<void,E>` default-ctor engaged（`Result<void>()` = success）
2. 值/错误构造 OK（`expected<T,E>(std::move(v))` / `(tl::unexpect, e)`）
3. 值语义: copy-constructible/assignable/nothrow-move；move-only unique_ptr
4. 误用 value() 抛 bad_expected_access，catch(std::exception) 兼容；what() 通用文案，细节走 .error()
5. 三头仅 variant（被移除）+ [[nodiscard]]（警告级）为 C++17 源 → 测试全量可 -std=c++11
6. cxxkit `3rdparty/<lib>/` 布局实证；来源 .refinfo/cxxkit/3rdparty/expected-1.2.0.tar.gz（git-ignored → vendor 副本必须提交）

## 设计定稿

### D1 vendor 布局（镜像 cxxkit）
`bindings/cxx/include/mediaservo/3rdparty/tl/expected.hpp`（原样 unmodified）+ `3rdparty/NOTICE`（版本/来源/不可修改）

### D2 共享头（~20 行, alias 方案, nodiscard 宏弃用）
`bindings/cxx/include/mediaservo/detail/result.hpp`:
```cpp
#include <mediaservo/3rdparty/tl/expected.hpp>
namespace mediaservo {
struct Error { int code; std::string message; };
template <typename T> using Result = tl::expected<T, Error>;
template <> using Result<void> = tl::expected<void, Error>;
}
```
nodiscard 宏弃用理由: 属性不能修饰 alias；tl::expected 自带 TL_EXPECTED_NODISCARD（C++17+）已覆盖。

### D3 改写模式表（三头 + 三测试全站, 唯一事实源）
| 现 API | 迁移后 |
|---|---|
| `Result<T>::success(v)` | `Result<T>(std::move(v))` |
| `Result<T>::failure(e)` | `Result<T>(tl::unexpect, e)` |
| `Result<void>::success()` | `Result<void>()` |
| `.ok()` / `.has_error()` | `.has_value()` / `!.has_value()` |
| `if (r)` / `if (!r)` | `if (r.has_value())` / `if (!r.has_value())` |
| catch std::logic_error（误用） | catch tl::bad_expected_access<Error> 或 std::exception（细节走 e.error()） |
| `r.value()` / `.error().code/.message` | 不变（异常类型变; Error 结构保留） |
| 头注释误用契约 | 改 bad_expected_access + 变更说明 |

### D4 测试标准 — 全量 `-std=c++11`

## 执行步骤

- **T1** vendor: 3rdparty/tl/expected.hpp + NOTICE（解包 .refinfo/cxxkit/3rdparty/expected-1.2.0.tar.gz）
  verify: `g++ -std=c++11 -fsyntax-only -I bindings/cxx/include ...` 零错零警 + 版本 grep
- **T2** 共享头 detail/result.hpp（双标准 C++11+C++17 smoke 编译）
- **T3** 契约测试 bindings/cxx/tests/test_result_common.cpp（RED 预期）: static_assert 值语义/alias 接线/跨 SDK is_same + 运行时 + bad_expected_access + 多 SDK 单 TU 回归
- **T4** 全站迁移（**原子步骤**）: 三头删本地实现 + include 共享头 + using + 全站改写（field 19/link 39/deck 50 站点）+ 三测试同步 + test-cxx.sh（-std=c++11 + -I bindings/cxx/include + 第 4 块契约测试）
  verify: test-cxx 4 PASS + 零残留 grep（`.ok()\|::success(\|std::logic_error` 全站 0 输出）+ `std::variant` 0
- **T5** parity_bindings.sh + e2e-bindings.sh: -std=c++11 + -I（e2e C++ 步骤真 server 推流 = 最强消费验证）
- **T6** install bindings: rglob 复制 bindings/cxx/include/（detail/ + 3rdparty/ + NOTICE）; verify: 安装后三文件存在
- **T7** 文档 23-binding-guide: 结构树/编译命令/C++11 承诺/契约变更声明/CC0 归属/未来 std::expected swap 路径
- **T8** 全门禁: build-c + test-cxx + parity-bindings + abi-drift + 安装前缀消费方 C++11 冒烟; e2e-bindings（需 server）
- **T9** 单次原子 commit（T1-T8 全内容）

## 依赖图

```
T1 → T2 → T3(RED) → T4(原子: T4a×3 ∥ T4b×3 ∥ T4c) → T5(2 并行) → T6 ∥ T7 → T8 → T9(单 commit)
```

## 风险登记

| 风险 | 缓解 |
|---|---|
| 异常契约变更 | 头注释明示 + 文档声明 + catch(std::exception) 兼容实证; 零消费者 |
| 工厂删除致站点遗漏 | T4 verify 零残留 grep 全站 0 输出 |
| what() 非 Error.message | 文档注明细节走 e.error().code/.message; 契约测试只断言 catch 类型 |
| alias 无 nodiscard | tl 内置 TL_EXPECTED_NODISCARD |
| 多 SDK 单 TU | T3 契约测试 include 三头回归 |
| 安装布局缺层次 | T6 rglob + mkdir + T8 消费方冒烟 |
| vendor 漂移 | NOTICE 不可修改声明 + T1 版本 grep; 升级走重新 vendor |
