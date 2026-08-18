// mediaservo-field-cxx 推流流程示例（骨架；需运行中 server 才能全流程跑通）。
// 编译示例: g++ -std=c++17 -I bindings/cxx/mediaservo-field-cxx/include -I bindings/c/mediaservo-field-c/include
//           -I bindings/c/include examples/vehicle_field.cpp -L target/debug -lmediaservo_field

#include <iostream>
#include <string>

#include "mediaservo/field.hpp"

int main() {
    using mediaservo::field::PushConfig;
    using mediaservo::field::PushSession;

    // 配置从环境变量读取（禁止硬编码密钥/地址）
    const char* url = std::getenv("MEDIASERVO_SIGNAL_URL");
    const char* psk = std::getenv("MEDIASERVO_PSK");
    const char* room = std::getenv("MEDIASERVO_ROOM");
    if (!url || !psk || !room) {
        std::cerr << "set MEDIASERVO_SIGNAL_URL / MEDIASERVO_PSK / MEDIASERVO_ROOM\n";
        return 1;
    }

    PushConfig cfg;
    cfg.url = url;
    cfg.psk = psk;
    cfg.room = room;

    auto session = PushSession::connect(cfg);
    if (!session) {
        std::cerr << "connect failed: code=" << session.error().code
                  << " msg=" << session.error().message << "\n";
        return 1;
    }
    auto s = std::move(session).value();

    auto track = s.publish_video();
    if (!track) {
        std::cerr << "publish failed: code=" << track.error().code
                  << " msg=" << track.error().message << "\n";
        return 1;
    }
    std::cout << "published track: " << track.value() << "\n";

    auto started = s.start_video_frames();
    if (!started) {
        std::cerr << "start frames failed: code=" << started.error().code << "\n";
        return 1;
    }

    std::cout << "pushing frames (Ctrl-C to stop)...\n";
    std::cin.get(); // 阻塞直到用户停止

    s.stop_video_frames();
    auto closed = s.close();
    if (!closed) {
        std::cerr << "close failed: code=" << closed.error().code << "\n";
        return 1;
    }
    return 0;
}
