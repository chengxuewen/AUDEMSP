---
name: test-harness
description: "MediaServo 多语言自动化测试工具架。从 SDD 规范生成测试骨架 (Rust/TS/Python/C++/C)、AAA 模式强制执行、测试→规范反向追溯、覆盖率报告。交互式菜单驱动。支持 Phase 感知 (跳过未就绪模块)。"
---

# 测试工具架 (Test Harness)

为 MediaServo 多语言项目提供自动化测试生成与验证。从 SDD 规范直接产出测试代码，确保 AAA 模式、Phase 对齐、语言惯例一致。

**哲学**: 测试不是写完代码再补的东西，是从规范直接长出来的。一个好测试文件 = 规范的可执行副本。

---

## 入口：测试任务类型

### `/test-harness`（无参数）
弹出任务类型菜单：

```
[1] SDD→测试 — 从规范生成测试文件 (stubs / AAA骨架 / 完整填充)
[2] 测试→规范 — 反向追溯 (验证覆盖 / 缺失场景)
[3] 用例执行 — 运行测试并修复失败用例
[4] 增量测试 — 从当前 git diff 产生增量测试
[5] 项目初始化 — 为新模块搭建测试基础设施
[6] 覆盖率报告 — 生成覆盖分析与缺口
[7] 选择性测试 — 仅运行变更影响的测试 (git diff 驱动)
```

### `/test-harness generate`
跳过菜单，直接进入 SDD→测试生成模式。

### `/test-harness run`
跳过菜单，直接运行测试。

### `/test-harness quick`
仅修复当前失败的测试，不生成新测试。

---

## 多语言策略

MediaServo 是一个多语言项目 (D19, D21)。测试生成必须适配各语言惯例。

### 语言检测

自动从项目结构检测目标语言：

| 信号 | 判定语言 |
|------|---------|
| `Cargo.toml` 存在 | Rust |
| `package.json` + `tsconfig.json` | TypeScript |
| `pyproject.toml` / `setup.py` | Python |
| `CMakeLists.txt` + `.cpp`/`.hpp` | C++ |
| `CMakeLists.txt` + `.c`/`.h` (无 .cpp) | C |
| 多信号 | 交互选择 |

### 各语言测试惯例

#### Rust
- **单元测试**: `#[cfg(test)] mod tests { ... }` 内联于源文件
- **集成测试**: `tests/` 目录，每个文件独立 crate
- **Mock**: `mockall` crate，`mock!` 宏
- **断言**: `assert_eq!` / `assert!` / `assert!(matches!(...))`
- **运行**: `cargo test`
- **惯例**:
  - 函数命名: `test_<module>_<scenario>`
  - 禁止 `unwrap()` / `expect()` — 使用 `?` 或 `assert!(result.is_ok())`
  - 测试模块 `use super::*;`

#### TypeScript
- **单元测试**: `*.test.ts` 或 `*.spec.ts` 与源文件同目录
- **框架**: Vitest (首选) / Jest
- **Mock**: `vi.mock()` / `jest.mock()`
- **断言**: `expect(x).toBe(y)` / `expect(x).toEqual(y)`
- **运行**: `npx vitest` / `npx jest`
- **惯例**:
  - `describe('ModuleName', () => { it('should ...', () => { ... }) })`
  - AAA 注释: `// Arrange` / `// Act` / `// Assert`

#### Python
- **单元测试**: `test_*.py` 与源文件同目录或 `tests/`
- **框架**: pytest
- **Mock**: `unittest.mock` / `pytest-mock`
- **断言**: 纯 `assert` (pytest 风格)
- **运行**: `pytest`
- **惯例**:
  - 函数命名: `test_<module>_<scenario>`
  - 类组织: `class Test<Module>:`
  - Fixture: `@pytest.fixture`

#### C++
- **单元测试**: `*_test.cpp` 在 `tests/` 目录
- **框架**: GoogleTest (gtest/gmock)
- **Mock**: `MOCK_METHOD` 宏
- **断言**: `EXPECT_EQ` / `ASSERT_TRUE`
- **运行**: `ctest` 或 `cmake --build build && ctest --test-dir build`
- **惯例**:
  - `TEST(TestSuiteName, TestName) { ... }`
  - AAA 注释

#### C
- **单元测试**: `test_*.c` 在 `tests/` 目录
- **框架**: Unity / CMock
- **运行**: `ctest` 或 `make test`
- **惯例**:
  - `TEST_ASSERT_EQUAL(expected, actual)`
  - `setUp() / tearDown()` 生命周期

---

## 工作流

### 模式 1: SDD → 测试生成

```
/test-harness generate
```

#### 生成层级 (每次询问)

1. **stubs**: 仅函数签名 + `todo!()` / `fail()` / `pytest.fail()` — 编译通过，测试失败
2. **AAA 骨架**: Arrange/Act/Assert 注释 + 占位 → 结构就绪，断言待填
3. **完整填充**: 从规范提取具体值，断言完整可运行 — 预期直接通过

#### Step 1: 识别规范源

自动扫描 `openspec/specs/` 目录：
```
openspec/specs/
├── hal-type-system-spec.md       → 30 项 (S-TYPE-*)
├── hal-qos-spec.md               → 30 项 (S-QOS-*)
├── config-barrier-spec.md        → 24 项 (S-CB-*)
└── hal-protocol-spec.md          → 37 项 (S-PROTO-*)
```

让用户选择规范文件 (单选或多选)。

#### Step 2: Phase 感知过滤

读取 `docs/plans/p0-milestone-roadmap.md`，判定当前 Phase：

| Phase | 可用规范 |
|-------|---------|
| Phase 0 (CI) | 类型系统 (S-TYPE) — 纯逻辑，无 trait 依赖 |
| Phase 1 (hal-core) | S-TYPE + S-QOS + S-CB + S-PROTO — trait 就绪 |
| Phase 2+ | 全部 |

自动过滤规范项：
- 当前 Phase 未就绪的规范 → 标记 ⏭️ 跳过，生成注释说明
- A 优先级 (P0) → 优先生成，完整填充
- B/B+/C 优先级 → stubs 或 AAA 骨架

#### Step 3: 生成测试文件

对每个规范项，生成对应语言的测试函数：

```
输入: S-TYPE-001
  - 前置条件: Bool = true / false
  - 操作: 编码 → 解码
  - 期望: true ↔ true, false ↔ false
  - 测试映射: test_type_01_bool_roundtrip

输出 (Rust):
  #[test]
  fn test_type_01_bool_roundtrip() {
      // Arrange
      let values = vec![true, false];
      // Act
      for val in values {
          let encoded = encode_bool(val);
          let decoded = decode_bool(&encoded);
          // Assert
          assert_eq!(val, decoded, "Bool roundtrip failed");
      }
  }
```

#### Step 4: 写入并验证

1. 写入测试文件 (单元测试内联 / 集成测试新文件)
2. 运行编译检查 (无语法错误)
3. 运行测试 (预期: 部分通过，部分失败 → 标记待实现)
4. 报告: `生成 N 个测试函数 → M 通过 / K 失败 / P 跳转`

---

### 模式 2: 测试 → 规范反向追溯

```
/test-harness trace
```

检查现有测试覆盖，与 SDD 规范交叉比对：

1. 扫描所有测试文件
2. 提取测试函数名 → 映射到规范 ID (如 `test_type_01_*` → S-TYPE-001)
3. 产生覆盖矩阵:

```
规范 ID     | 测试函数                            | 状态
-----------|------------------------------------|------
S-TYPE-001 | test_type_01_bool_roundtrip        | ✅
S-TYPE-002 | test_type_02_s8_roundtrip          | ✅
S-TYPE-003 | —                                  | ❌ 缺失
S-CB-007   | test_cb_07_partial_failure          | ⚠️ 不完整
```

4. 标记:
   - ❌ 缺失 → 推荐从 SDD 生成
   - ⚠️ 不完整 → 边界条件未覆盖
   - ✅ 完整

---

### 模式 3: 用例执行与修复

```
/test-harness run
```

1. 运行当前项目测试套件
2. 收集失败列表
3. 对每个失败:
   - 读取测试源码
   - 读取对应实现源码
   - 判定根因: 测试错误 vs 实现错误
   - 自动修复测试错误 (错误断言、缺失 mock)
   - 标记实现错误 → 报告给用户

**根因判定规则**:
- 测试函数逻辑与规范不一致 → 测试错误
- 断言值错误 (期望 1 但规范说应为 2) → 测试错误
- 实现未完成 / 接口变更 → 实现错误

**绝不**: 修改实现代码来让测试通过 (除非用户明确要求)。

---

### 模式 4: 增量测试

```
/test-harness incremental
```

从 `git diff` 识别变更 → 生成对应测试：

1. `git diff --name-only` 获取变更文件
2. 反向映射到 SDD 规范 (文件路径 → 模块 → 规范 ID)
3. 仅对变更相关的规范生成增量测试
4. 如果变更文件还没有测试 → 初始化测试文件
（注：Mode 7 用于运行受影响测试，Mode 4 用于生成增量测试）

---

### 模式 5: 项目初始化

```
/test-harness init
```

为新模块创建测试基础设施:

**Rust**:
- `tests/` 目录 + 集成测试入口
- crate-level `#[cfg(test)]` helper 模块
- `mockall` 依赖检查

**TypeScript**:
- `vitest.config.ts` / `jest.config.ts`
- `__tests__/` 目录
- test setup 文件

**Python**:
- `tests/__init__.py` + `conftest.py`
- pytest 配置 (pyproject.toml)

**C++**:
- `CMakeLists.txt` 测试目标
- `tests/` 目录 + CMakeLists.txt
- gtest 集成

**C**:
- `CMakeLists.txt` 测试目标
- Unity/CMock 框架下载

---

### 模式 6: 覆盖率报告

```
/test-harness coverage
```

1. 运行带覆盖率的测试
2. 按模块聚合并展示:

```
模块                | 行覆盖    | 分支覆盖   | 规范覆盖
-------------------|----------|----------|----------
hal-type-system    | 92%      | 85%      | 28/30
hal-qos            | 78%      | 71%      | 22/30
config-barrier     | 65%      | 58%      | 18/24
hal-protocol       | 88%      | 82%      | 34/37
───────────────────|──────────|──────────|────────
总计               | 81%      | 74%      | 102/121
```

3. 缺口排序: 未覆盖规范项 / 低分支覆盖函数
4. 推荐: 优先补充的 N 项测试

---

### 模式 7: 选择性测试运行 (Selective Test Runner)

```
/test-harness selective
```

> 注：mediaservo 和 hal-flatbuffers 为 Phase 1 stub crates（M0.3），当前无实际测试目标。Mode 7 使用 `cargo metadata` 检查 crate 存在性后再建议测试命令。
从 `git diff` 识别变更 → 仅运行受影响的测试（非门禁，调度工具）：

1. `git diff --name-only HEAD` 获取变更文件
2. 映射文件到测试目标:

| 变更路径匹配 | 测试命令 |
|---|---|
| `crates/mediaservo-common/` | `cargo test -p mediaservo-common` |
| `crates/mediaservo/` | `cargo test -p mediaservo`（Phase 1, stub only） |
| `crates/hal-flatbuffers/` | `cargo test -p hal-flatbuffers`（Phase 1, stub only） |
| `*.fbs` (FlatBuffers schema) | `cargo test -p hal-flatbuffers` |
| `Cargo.toml` or `Cargo.lock` | `cargo test --workspace` |
| 跨 crate (3+ crates) | `./scripts/qa-fast.sh` |

3. 运行测试（一次一层，避免 cargo 锁冲突）
4. 报告: 层 / 命令 / 测试数 / 通过 / 失败 / 耗时

**绝不**: 同时运行 cargo test 多个 crate。不诊断失败 — 仅报告。>20 文件变更 → 建议 `cargo test --workspace`。

---

## MediaServo 特定测试模式

### HAL Trait 测试

所有 HAL 核心 trait 测试使用 `MockHalTransport` (`mediaservo`)：

```rust
// Rust 模式
#[test]
fn test_signal_write_read() {
    // Arrange
    let mut transport = MockHalTransport::new();
    let signal = Signal::new("test.value", HalValue::S32(42));

    // Act
    transport.write_signal(&signal).unwrap();
    let result = transport.read_signal("test.value").unwrap();

    // Assert
    assert_eq!(result, HalValue::S32(42));
}
```

### HalQoS 安全域标签

```rust
#[test]
fn test_qos_security_domain_hierarchical() {
    let tag = SecurityDomain::parse("l1.control.reactor_a").unwrap();
    assert!(tag.matches("l1.*"));
    assert!(tag.matches("l1.control.*"));
    assert!(!tag.matches("l2.*"));
}
```

### Config Barrier 状态机

```rust
#[test]
fn test_config_barrier_state_machine() {
    let mut barrier = ConfigBarrier::new();
    assert_eq!(barrier.state(), BarrierState::Idle);

    // Queue config change
    barrier.queue(ConfigChange::UpdateSignal { ... });
    assert_eq!(barrier.state(), BarrierState::Pending);

    // Apply at cycle boundary
    barrier.apply().unwrap();
    assert_eq!(barrier.state(), BarrierState::Idle);
}
```

### FlatBuffers Round-Trip

```rust
#[test]
fn test_halvalue_fbs_roundtrip() {
    let original = HalValue::S32(42);
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let offset = original.serialize(&mut builder);
    builder.finish(offset, None);
    let buf = builder.finished_data();
    let restored = HalValue::deserialize(buf).unwrap();
    assert_eq!(original, restored);
}
```

---

## 交互模式

所有测试生成操作在写入文件前展现 diff 预览，由用户确认：

```
将生成以下变更:
  crates/mediaservo-common/src/types.rs +45 (内联测试模块)
  tests/integration/test_type_roundtrip.rs (新文件, 150行)
  tests/integration/test_qos_security.rs (新文件, 80行)

总计: 3 文件, +275 行, 30 测试函数

执行? [Y/n]
```

---

## Phase 感知规则

自动读取 `docs/plans/p0-milestone-roadmap.md` 确认当前里程碑。

| 检查点 | 行为 |
|--------|------|
| 规范所需 trait 尚未定义 | 生成 stub 或跳过，标注 "⏭️ Phase 2" |
| 优先级 A (P0 必须) | 完整填充 + 断言 |
| 优先级 B/B+ | AAA 骨架 |
| 优先级 C | stub 占位 |

Phase 变化时，重新运行 `generate` → 之前跳过的测试自动填充。

---

## 质量规则

1. **每个测试一个断言目标** — 一个 test 函数验证一个规范项
2. **AAA 注释必须显式** — `// Arrange` / `// Act` / `// Assert` 不可省略
3. **不修改测试让实现通过** — 规范 > 测试 > 实现 (优先级)
4. **禁止 unwrap/expect** — 测试中也避免，用 `assert!(result.is_ok())`
5. **边界条件优先** — 规范中 `边界条件` 字段的每个条目 → 独立测试
6. **命名可追溯** — 测试函数名包含规范 ID (如 `test_type_01_*`)
7. **Phase 对齐** — 不生成当前 Phase 无法运行的测试

---

## 快速参考

```
/test-harness               → 选择任务类型
/test-harness generate      → SDD→测试生成
/test-harness run           → 运行并修复测试
/test-harness trace         → 测试→规范反向追溯
/test-harness incremental   → 增量测试 (git diff)
/test-harness init          → 初始化测试基础设施
/test-harness coverage      → 覆盖率报告
/test-harness selective    → 选择性测试运行 (git diff)
```

### 语言命令速查

| 语言 | 运行测试 | 覆盖率 | Mock 库 |
|------|---------|--------|---------|
| Rust | `cargo test` | `cargo tarpaulin` | mockall |
| TS | `npx vitest` | `npx vitest --coverage` | vi.mock() |
| Python | `pytest` | `pytest --cov` | pytest-mock |
| C++ | `ctest` | `gcov + lcov` | gmock |
| C | `ctest` | `gcov + lcov` | CMock |
