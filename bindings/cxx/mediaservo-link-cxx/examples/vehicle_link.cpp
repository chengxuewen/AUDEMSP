// mediaservo-link-cxx 信令 + 事件示例（骨架；需运行中 server 才能全流程跑通）。
// 编译示例: g++ -std=c++17 -I bindings/cxx/mediaservo-link-cxx/include -I bindings/c/mediaservo-link-c/include
//           -I bindings/c/include examples/vehicle_link.cpp -L target/debug -lmediaservo_link

#include <cstdlib>
#include <iostream>
#include <string>

#include "mediaservo/link.hpp"

int main() {
    using mediaservo::link::SignalConfig;
    using mediaservo::link::SignalSession;

    // 配置从环境变量读取（禁止硬编码密钥/地址）
    const char* url = std::getenv("MEDIASERVO_SIGNAL_URL");
    const char* psk = std::getenv("MEDIASERVO_PSK");
    const char* room = std::getenv("MEDIASERVO_ROOM");
    if (!url || !psk || !room) {
        std::cerr << "set MEDIASERVO_SIGNAL_URL / MEDIASERVO_PSK / MEDIASERVO_ROOM\n";
        return 1;
    }

    SignalConfig cfg;
    cfg.url = url;
    cfg.psk = psk;
    cfg.room = room;
    cfg.role = "Pusher"; // 车端推流角色

    // 先注册事件回调再发连接？—— 回调在 connect 后任意时刻注册即可；
    // 首次注册时泵合成补发 {"type":"connected"} 事件（C ABI 契约）。
    auto result = SignalSession::connect(cfg);
    if (!result) {
        std::cerr << "connect failed: code=" << result.error().code
                  << " msg=" << result.error().message << "\n";
        return 1;
    }
    auto session = std::move(result).value();

    session.on_event([](const std::string& event_json) {
        std::cout << "event: " << event_json << "\n"; // 事件串仅在回调内有效（已拷贝）
    });

    auto sent = session.send("{\"type\":\"get_status\"}");
    if (!sent) {
        std::cerr << "send failed: code=" << sent.error().code
                  << " msg=" << sent.error().message << "\n";
        return 1;
    }

    std::cout << "waiting for events (Ctrl-C to stop)...\n";
    std::cin.get();

    auto closed = session.close();
    if (!closed) {
        std::cerr << "close failed: code=" << closed.error().code << "\n";
        return 1;
    }
    return 0;
}
