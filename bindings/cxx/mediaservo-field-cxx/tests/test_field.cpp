// mediaservo-field-cxx 编译运行测试：version + 错误路径 + move 语义 + Result 误用。
// 编译示例: g++ -std=c++17 -I bindings/cxx/mediaservo-field-cxx/include -I bindings/c/mediaservo-field-c/include
//           -I bindings/c/include tests/test_field.cpp -L target/debug -lmediaservo_field
//           -o /tmp/opencode/test_field_cxx

#include <cassert>
#include <stdexcept>
#include <string>

#include "mediaservo/field.hpp"

using mediaservo::field::PushConfig;
using mediaservo::field::PushSession;

static void test_version() {
    auto v = mediaservo::field::version();
    assert(v.ok());
    assert(v.value().rfind("0.1.", 0) == 0); // "0.1.x"
}

static void test_connect_error_path() {
    // 空配置 → C ABI 快速 INVALID_ARG（url/psk/room 必填），不触网
    auto r = PushSession::connect(PushConfig{});
    assert(!r.ok());
    assert(r.has_error());
    assert(r.error().code == MEDIASERVO_FIELD_ERR_INVALID_ARG);
    assert(!r.error().message.empty());
}

static void test_result_misuse_throws() {
    auto r = PushSession::connect(PushConfig{});
    assert(!r.ok());
    bool threw = false;
    try {
        (void)r.value();
    } catch (const std::logic_error&) {
        threw = true;
    }
    assert(threw && "value() on error must throw std::logic_error");
}

static void test_closed_session_error_path() {
    PushSession s; // 默认构造 = 已关闭
    assert(!s);

    auto p = s.publish_video();
    assert(!p.ok());
    assert(p.error().code == MEDIASERVO_FIELD_ERR_INVALID_ARG);
    assert(p.error().message == "closed");

    auto st = s.start_video_frames();
    assert(!st.ok());
    assert(st.error().code == MEDIASERVO_FIELD_ERR_INVALID_ARG);

    s.stop_video_frames(); // 已关闭 no-op，不崩
    assert(s.close().ok()); // 幂等
    assert(s.close().ok());
}

static void test_move_semantics() {
    PushSession a;                       // null
    PushSession b(std::move(a));         // move 构造
    assert(!a && !b);

    PushSession c;
    c = std::move(b);                    // move 赋值
    assert(!b && !c);

    (void)c.close();                     // double-close 安全（析构再触发一次）
}

int main() {
    test_version();
    test_connect_error_path();
    test_result_misuse_throws();
    test_closed_session_error_path();
    test_move_semantics();
    return 0;
}
