// mediaservo-link-cxx 编译运行测试：version + 错误路径 + move 语义 + Result 误用 + 回调生命周期。
// 编译示例: g++ -std=c++17 -I bindings/cxx/mediaservo-link-cxx/include -I bindings/c/mediaservo-link-c/include
//           -I bindings/c/include tests/test_link.cpp -L target/debug -lmediaservo_link
//           -o /tmp/opencode/test_link_cxx

#include <cassert>
#include <stdexcept>
#include <string>
#include <vector>

#include "mediaservo/link.hpp"

using mediaservo::link::Bus;
using mediaservo::link::SignalConfig;
using mediaservo::link::SignalSession;

static void test_version() {
    auto v = mediaservo::link::version();
    assert(v.ok());
    assert(v.value().rfind("0.1.", 0) == 0); // "0.1.x"
}

static void test_signal_connect_error_path() {
    // 空配置 → C ABI 快速 INVALID_ARG（url/psk/room 必填），不触网
    auto r = SignalSession::connect(SignalConfig{});
    assert(!r.ok());
    assert(r.error().code == MEDIASERVO_LINK_ERR_INVALID_ARG);
    assert(!r.error().message.empty());
}

static void test_result_misuse_throws() {
    auto r = SignalSession::connect(SignalConfig{});
    assert(!r.ok());
    bool threw = false;
    try {
        (void)r.value();
    } catch (const std::logic_error&) {
        threw = true;
    }
    assert(threw && "value() on error must throw std::logic_error");

    auto ok_r = mediaservo::link::version();
    assert(ok_r.ok());
    threw = false;
    try {
        (void)ok_r.error();
    } catch (const std::logic_error&) {
        threw = true;
    }
    assert(threw && "error() on success must throw std::logic_error");
}

static void test_closed_signal_error_path() {
    SignalSession s; // 默认构造 = 已关闭
    assert(!s);

    auto send = s.send("{\"type\":\"ping\"}");
    assert(!send.ok());
    assert(send.error().code == MEDIASERVO_LINK_ERR_INVALID_ARG);
    assert(send.error().message == "closed");

    s.on_event([](const std::string&) {}); // 已关闭 no-op，不崩
    assert(s.close().ok()); // 幂等
    assert(s.close().ok());
}

static void test_bus_attach_error_path() {
    // 空 token/vk → JWT 验签快速失败（BUS 错误，不建 iceoryx 节点）
    auto r = Bus::attach("", "", "");
    assert(!r.ok());
    assert(r.error().code == MEDIASERVO_LINK_ERR_BUS);
    assert(!r.error().message.empty());
}

static void test_closed_bus_error_path() {
    Bus b; // 默认构造 = 已关闭
    assert(!b);

    mediaservo_frame_meta_t meta{};
    auto p = b.publish("camera/0", std::vector<uint8_t>{}, meta);
    assert(!p.ok());
    assert(p.error().code == MEDIASERVO_LINK_ERR_INVALID_ARG);
    assert(p.error().message == "closed");

    auto sub = b.subscribe("camera/0");
    assert(!sub.ok());
    assert(sub.error().code == MEDIASERVO_LINK_ERR_INVALID_ARG);

    assert(b.close().ok());
    assert(b.close().ok());
}

static void test_closed_stream_error_path() {
    mediaservo::link::Stream st; // 默认构造 = 已关闭
    assert(!st);

    auto f = st.recv();
    assert(!f.ok());
    assert(f.error().code == MEDIASERVO_LINK_ERR_INVALID_ARG);
    assert(f.error().message == "closed");

    assert(st.close().ok());
    assert(st.close().ok());
}

static void test_move_semantics() {
    SignalSession a;
    SignalSession b(std::move(a));
    assert(!a && !b);

    SignalSession c;
    c = std::move(b);
    assert(!b && !c);
    (void)c.close();

    Bus x;
    Bus y(std::move(x));
    assert(!x && !y);
    y = std::move(x);
    assert(!x && !y);
    (void)y.close();

    mediaservo::link::Stream s1;
    mediaservo::link::Stream s2(std::move(s1));
    assert(!s1 && !s2);
    (void)s2.close();
}

int main() {
    test_version();
    test_signal_connect_error_path();
    test_result_misuse_throws();
    test_closed_signal_error_path();
    test_bus_attach_error_path();
    test_closed_bus_error_path();
    test_closed_stream_error_path();
    test_move_semantics();
    return 0;
}
