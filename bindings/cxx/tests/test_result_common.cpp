// mediaservo 共享 Result 契约测试（C++11 起步，tl::expected 原生 API）。
// 前置: bindings/cxx/include（共享头）+ 三 SDK 头 + C 头（-I 列表见 test-cxx.sh）。
// 覆盖: 值语义 static_assert / alias 接线 / 跨 SDK 类型同一性 / 运行时路径 / 误用契约。
// 本文件是未来 std::expected swap 的回归锚（result.hpp 两行 alias 即全量迁移）。

#include <cassert>
#include <memory>
#include <string>
#include <type_traits>
#include <utility>

#include <mediaservo/field.hpp>
#include <mediaservo/link.hpp>
#include <mediaservo/deck.hpp>

// ── static_assert: 值语义 ────────────────────────────────────
static_assert(std::is_copy_constructible<mediaservo::Result<int> >::value, "Result<int> copy-constructible");
static_assert(std::is_copy_assignable<mediaservo::Result<int> >::value, "Result<int> copy-assignable");
static_assert(std::is_nothrow_move_constructible<mediaservo::Result<int> >::value, "Result<int> nothrow-move");
static_assert(std::is_move_constructible<mediaservo::Result<std::unique_ptr<int> > >::value, "move-only T: move ok");
static_assert(!std::is_copy_constructible<mediaservo::Result<std::unique_ptr<int> > >::value, "move-only T: no copy");

// ── static_assert: alias 接线（Result = tl::expected<T, Error>）──
static_assert(std::is_same<mediaservo::Result<int>,
                           tl::expected<int, mediaservo::Error> >::value, "alias -> tl::expected");
static_assert(std::is_same<mediaservo::Result<void>,
                           tl::expected<void, mediaservo::Error> >::value, "alias void -> tl::expected");

// ── static_assert: 跨 SDK 类型同一性（using 接线 + 多 SDK 单 TU）──
static_assert(std::is_same<mediaservo::field::Error, mediaservo::link::Error>::value, "field/link Error same");
static_assert(std::is_same<mediaservo::field::Error, mediaservo::deck::Error>::value, "field/deck Error same");
static_assert(std::is_same<mediaservo::field::Result<int>, mediaservo::link::Result<int> >::value, "field/link Result same");
static_assert(std::is_same<mediaservo::field::Result<void>, mediaservo::deck::Result<void> >::value, "field/deck Result<void> same");

// ── 运行时: 成功/失败/访问 ───────────────────────────────────
int main() {
    // 成功路径
    mediaservo::Result<int> a(42);
    assert(a.has_value());
    assert(a.value() == 42);
    assert(a.value_or(-1) == 42); // value_or 成功取 value

    // 失败路径
    mediaservo::Result<int> b(tl::unexpect, mediaservo::Error{-1, "boom"});
    assert(!b.has_value());
    assert(b.error().code == -1);
    assert(b.error().message == "boom");
    assert(b.value_or(-1) == -1); // value_or 失败取缺省

    // 移动保语义
    mediaservo::Result<int> c(std::move(b));
    assert(!c.has_value());
    assert(c.error().message == "boom");

    // Result<void> 双路径
    mediaservo::Result<void> v; // default-ctor = success（实证）
    assert(v.has_value());
    mediaservo::Result<void> ve(tl::unexpect, mediaservo::Error{-2, "void err"});
    assert(!ve.has_value());
    assert(ve.error().message == "void err");

    bool threw = false; // 误用契约（error() on success 为 UB 标准语义，不断言）
    try {
        (void)b.value(); // 失败态调 value() -> throw
    } catch (const tl::bad_expected_access<mediaservo::Error>&) {
        threw = true;
    }
    assert(threw);

    // catch(std::exception) 兼容性（契约变更声明: 旧 logic_error 也是 std::exception 子类）
    threw = false;
    try {
        (void)b.value();
    } catch (const std::exception& e) {
        threw = true;
        (void)e;
    }
    assert(threw);

    return 0;
}
