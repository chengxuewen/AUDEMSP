/* MediaServo link C++ header-only binding (信令 + 帧总线) — 设备侧 C++ 消费。
 *
 * 薄包装 over bindings/c/mediaservo-link-c/include/mediaservo/link.h (D247)。
 * 生命周期契约（自 C ABI 头翻译成 RAII 保证）：
 *   - SignalSession/Bus/Stream 均 move-only；析构自动 close（幂等）。
 *   - 默认构造 = 已关闭（null handle）；对已关闭对象调用 API 返回
 *     Error{INVALID_ARG, "closed"}，不触碰 C ABI。
 *   - 事件/帧回调（trampoline）在内部泵线程触发；std::function 堆对象在
 *     close（join 泵线程）后统一释放，防 use-after-free。回调内禁止调用
 *     close/on_event（C 契约）；重复注册不释放旧回调对象（泵线程可能正在
 *     执行它），随 close 一起释放 —— 注册次数通常为 1，上界有界。
 *   - 错误通道为 Result（非异常）；仅 value()/error() 误用抛 std::logic_error。
 */
#ifndef MEDIASERVO_LINK_HPP
#define MEDIASERVO_LINK_HPP

#pragma once

#include <cstdint>
#include <functional>
#include <stdexcept>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include <mediaservo/common.h>
#include <mediaservo/link.h>

namespace mediaservo {
namespace link {

/// 错误详情（code 为 mediaservo/link.h 中 MEDIASERVO_LINK_ERR_* 值；message 读自 last_error）。
struct Error {
    int code;
    std::string message;
};

namespace detail {

/// 从 C ABI last_error 构造错误（C 调用返回 <0 后调用）。
inline Error make_error(int code) {
    char buf[512];
    mediaservo_link_last_error(buf, sizeof(buf));
    return Error{code, std::string(buf)};
}

} // namespace detail

/// 成功或错误返回值（livekit Result 模式；误用 value()/error() 抛 std::logic_error）。
template <typename T>
class [[nodiscard]] Result {
public:
    static Result success(T value) { return Result(std::variant<T, Error>(std::in_place_index<0>, std::move(value))); }
    static Result failure(Error error) { return Result(std::variant<T, Error>(std::in_place_index<1>, std::move(error))); }

    bool ok() const noexcept { return storage_.index() == 0; }
    bool has_error() const noexcept { return !ok(); }
    explicit operator bool() const noexcept { return ok(); }

    T& value() & {
        if (!ok()) throw std::logic_error("Result::value() called on an error result");
        return std::get<0>(storage_);
    }
    const T& value() const& {
        if (!ok()) throw std::logic_error("Result::value() called on an error result");
        return std::get<0>(storage_);
    }
    T&& value() && {
        if (!ok()) throw std::logic_error("Result::value() called on an error result");
        return std::get<0>(std::move(storage_));
    }

    Error& error() & {
        if (ok()) throw std::logic_error("Result::error() called on a success result");
        return std::get<1>(storage_);
    }
    const Error& error() const& {
        if (ok()) throw std::logic_error("Result::error() called on a success result");
        return std::get<1>(storage_);
    }
    Error&& error() && {
        if (ok()) throw std::logic_error("Result::error() called on a success result");
        return std::get<1>(std::move(storage_));
    }

private:
    explicit Result(std::variant<T, Error> storage) : storage_(std::move(storage)) {}
    std::variant<T, Error> storage_;
};

/// void 特化（操作仅报告成败）。
template <>
class [[nodiscard]] Result<void> {
public:
    static Result success() { return Result(std::monostate{}); }
    static Result failure(Error error) { return Result(std::variant<Error, std::monostate>(std::in_place_index<0>, std::move(error))); }

    bool ok() const noexcept { return storage_.index() == 1; }
    bool has_error() const noexcept { return !ok(); }
    explicit operator bool() const noexcept { return ok(); }

    /// 校验成功；误用抛 std::logic_error。
    void value() const {
        if (!ok()) throw std::logic_error("Result::value() called on an error result");
    }

    Error& error() & {
        if (ok()) throw std::logic_error("Result::error() called on a success result");
        return std::get<0>(storage_);
    }
    const Error& error() const& {
        if (ok()) throw std::logic_error("Result::error() called on a success result");
        return std::get<0>(storage_);
    }
    Error&& error() && {
        if (ok()) throw std::logic_error("Result::error() called on a success result");
        return std::get<0>(std::move(storage_));
    }

private:
    Result(std::variant<Error, std::monostate> storage) : storage_(std::move(storage)) {}
    std::variant<Error, std::monostate> storage_;
};

/// SDK 版本 (MAJOR.MINOR.PATCH)。
inline Result<std::string> version() {
    char buf[64];
    int rc = mediaservo_link_version(buf, sizeof(buf));
    if (rc != MEDIASERVO_OK) {
        return Result<std::string>::failure(detail::make_error(rc));
    }
    return Result<std::string>::success(std::string(buf));
}

/// 信令配置（对应 mediaservo_link_signal_config_t）。
struct SignalConfig {
    std::string url;
    std::string psk;
    std::string room;
    /// "Host"/"Pusher" → Host, "Client"/"Puller" → Remote；空串 = Host。
    std::string role;
};

/// 信令会话（move-only RAII；析构自动 close；默认构造 = 已关闭）。
class SignalSession {
public:
    /// 连接信令 + 创建会话（阻塞；失败返回错误，不抛异常）。
    static Result<SignalSession> connect(const SignalConfig& cfg) {
        mediaservo_link_signal_config_t c = MEDIASERVO_LINK_SIGNAL_CONFIG_DEFAULT;
        c.url = cfg.url.empty() ? nullptr : cfg.url.c_str();
        c.psk = cfg.psk.empty() ? nullptr : cfg.psk.c_str();
        c.room = cfg.room.empty() ? nullptr : cfg.room.c_str();
        c.role = cfg.role.empty() ? nullptr : cfg.role.c_str();

        mediaservo_link_signal_t* h = nullptr;
        int rc = mediaservo_link_signal_connect(&c, &h);
        if (rc != MEDIASERVO_OK) {
            return Result<SignalSession>::failure(detail::make_error(rc));
        }
        return Result<SignalSession>::success(SignalSession(h));
    }

    /// 默认构造 = 已关闭会话。
    SignalSession() noexcept = default;
    ~SignalSession() { (void)close(); }

    SignalSession(const SignalSession&) = delete;
    SignalSession& operator=(const SignalSession&) = delete;
    SignalSession(SignalSession&& other) noexcept : h_(other.release_()), cbs_(std::move(other.cbs_)) {}
    SignalSession& operator=(SignalSession&& other) noexcept {
        if (this != &other) {
            (void)close();
            h_ = other.release_();
            cbs_ = std::move(other.cbs_);
        }
        return *this;
    }

    /// 是否持有有效会话。
    explicit operator bool() const noexcept { return h_ != nullptr; }

    /// 发送一条信令消息（JSON；SignalingMessage type 标签 snake_case）。
    Result<void> send(const std::string& json) {
        if (!h_) return Result<void>::failure(Error{MEDIASERVO_LINK_ERR_INVALID_ARG, "closed"});
        int rc = mediaservo_link_signal_send(h_, json.c_str(), json.size());
        if (rc != MEDIASERVO_OK) {
            return Result<void>::failure(detail::make_error(rc));
        }
        return Result<void>::success();
    }

    /// 注册事件回调（connect 后任意时刻；重复注册替换；回调在内部泵线程触发，
    /// 事件串仅在回调内有效，需保留请拷贝；回调内禁止调用本对象任何方法）。
    void on_event(std::function<void(const std::string&)> cb) {
        if (!h_) return; // 已关闭：no-op
        auto* f = new std::function<void(const std::string&)>(std::move(cb));
        cbs_.push_back(f); // ponytail: 旧回调对象留到 close 释放（泵线程可能正在执行，防 UAF）；
                           // 注册次数通常 1，上界 = 注册次数，有界。
        mediaservo_link_signal_on_event(h_, &SignalSession::trampoline, f);
    }

    /// 关闭会话并释放 handle（幂等；join 事件泵后才释放回调对象）。
    Result<void> close() noexcept {
        if (!h_ && cbs_.empty()) return Result<void>::success();
        int rc = MEDIASERVO_OK;
        if (h_) {
            rc = mediaservo_link_signal_close(h_);
            h_ = nullptr;
        }
        for (auto* cb : cbs_) delete cb;
        cbs_.clear();
        if (rc != MEDIASERVO_OK) {
            return Result<void>::failure(detail::make_error(rc));
        }
        return Result<void>::success();
    }

private:
    explicit SignalSession(mediaservo_link_signal_t* h) : h_(h) {}
    mediaservo_link_signal_t* release_() noexcept {
        mediaservo_link_signal_t* p = h_;
        h_ = nullptr;
        return p;
    }
    static void trampoline(mediaservo_link_signal_t*, const char* event_json, void* user) {
        (*static_cast<std::function<void(const std::string&)>*>(user))(std::string(event_json));
    }
    mediaservo_link_signal_t* h_ = nullptr;
    std::vector<std::function<void(const std::string&)>*> cbs_;
};

/// 帧总线（move-only RAII；析构自动 close）。
class Bus {
public:
    /// 附加帧总线（验签 + ACL + iceoryx2 节点，阻塞）。endpoint 为 Phase 1 预留（空串即可）。
    static Result<Bus> attach(const std::string& endpoint, const std::string& token_pem, const std::string& vk_pem) {
        mediaservo_link_bus_t* b = nullptr;
        int rc = mediaservo_link_bus_attach(endpoint.c_str(), token_pem.c_str(), vk_pem.c_str(), &b);
        if (rc != MEDIASERVO_OK) {
            return Result<Bus>::failure(detail::make_error(rc));
        }
        return Result<Bus>::success(Bus(b));
    }

    /// 默认构造 = 已关闭总线。
    Bus() noexcept = default;
    ~Bus() { (void)close(); }

    Bus(const Bus&) = delete;
    Bus& operator=(const Bus&) = delete;
    Bus(Bus&& other) noexcept : h_(other.release_()) {}
    Bus& operator=(Bus&& other) noexcept {
        if (this != &other) {
            (void)close();
            h_ = other.release_();
        }
        return *this;
    }

    /// 是否持有有效总线。
    explicit operator bool() const noexcept { return h_ != nullptr; }

    /// 发布一帧（ACL 检查 + SHM loan + send，阻塞）。
    Result<void> publish(const std::string& topic, const std::vector<uint8_t>& payload, const mediaservo_frame_meta_t& meta) {
        if (!h_) return Result<void>::failure(Error{MEDIASERVO_LINK_ERR_INVALID_ARG, "closed"});
        const uint8_t* data = payload.empty() ? nullptr : payload.data(); // C 契约：payload NULL 当且仅当 len==0
        int rc = mediaservo_link_bus_publish(h_, topic.c_str(), data, payload.size(), &meta);
        if (rc != MEDIASERVO_OK) {
            return Result<void>::failure(detail::make_error(rc));
        }
        return Result<void>::success();
    }

    /// 订阅 topic，创建帧流（阻塞）。
    Result<class Stream> subscribe(const std::string& topic);

    /// 关闭帧总线并释放 handle（幂等；shutdown 全部流，stream recv 返回 CLOSED）。
    Result<void> close() noexcept {
        if (!h_) return Result<void>::success();
        int rc = mediaservo_link_bus_close(h_);
        h_ = nullptr;
        if (rc != MEDIASERVO_OK) {
            return Result<void>::failure(detail::make_error(rc));
        }
        return Result<void>::success();
    }

private:
    explicit Bus(mediaservo_link_bus_t* h) : h_(h) {}
    mediaservo_link_bus_t* release_() noexcept {
        mediaservo_link_bus_t* p = h_;
        h_ = nullptr;
        return p;
    }
    mediaservo_link_bus_t* h_ = nullptr;
};

/// 一帧（元数据 + 载荷拷贝）。
struct Frame {
    mediaservo_frame_meta_t meta{};
    std::vector<uint8_t> data;
};

/// 帧流（move-only RAII；析构自动 close）。
class Stream {
public:
    /// 默认构造 = 已关闭流。
    Stream() noexcept = default;
    ~Stream() { (void)close(); }

    Stream(const Stream&) = delete;
    Stream& operator=(const Stream&) = delete;
    Stream(Stream&& other) noexcept : h_(other.release_()) {}
    Stream& operator=(Stream&& other) noexcept {
        if (this != &other) {
            (void)close();
            h_ = other.release_();
        }
        return *this;
    }

    /// 是否持有有效流。
    explicit operator bool() const noexcept { return h_ != nullptr; }

    /// 阻塞取帧（元数据 + 载荷拷贝）。
    Result<Frame> recv() {
        if (!h_) return Result<Frame>::failure(Error{MEDIASERVO_LINK_ERR_INVALID_ARG, "closed"});
        Frame frame;
        // ponytail: 单缓冲 16MiB 覆盖 4K I420（12.4MiB）；C ABI 无法探测截断
        //（meta 仅取帧后可知），更大帧需扩缓冲 —— 升级路径: 按 meta 缓存上次
        // 帧尺寸自适应增长。
        frame.data.resize(16 * 1024 * 1024);
        size_t len = 0;
        int rc = mediaservo_link_bus_recv(h_, &frame.meta, frame.data.data(), frame.data.size(), &len);
        if (rc != MEDIASERVO_OK) {
            return Result<Frame>::failure(detail::make_error(rc));
        }
        frame.data.resize(len);
        return Result<Frame>::success(std::move(frame));
    }

    /// 关闭帧流并释放 handle（幂等；唤醒阻塞中的 recv 使其返回 CLOSED）。
    Result<void> close() noexcept {
        if (!h_) return Result<void>::success();
        int rc = mediaservo_link_stream_close(h_);
        h_ = nullptr;
        if (rc != MEDIASERVO_OK) {
            return Result<void>::failure(detail::make_error(rc));
        }
        return Result<void>::success();
    }

private:
    friend class Bus;
    explicit Stream(mediaservo_link_stream_t* h) : h_(h) {}
    mediaservo_link_stream_t* release_() noexcept {
        mediaservo_link_stream_t* p = h_;
        h_ = nullptr;
        return p;
    }
    mediaservo_link_stream_t* h_ = nullptr;
};

inline Result<class Stream> Bus::subscribe(const std::string& topic) {
    if (!h_) return Result<Stream>::failure(Error{MEDIASERVO_LINK_ERR_INVALID_ARG, "closed"});
    mediaservo_link_stream_t* st = nullptr;
    int rc = mediaservo_link_bus_subscribe(h_, topic.c_str(), &st);
    if (rc != MEDIASERVO_OK) {
        return Result<Stream>::failure(detail::make_error(rc));
    }
    return Result<Stream>::success(Stream(st));
}

} // namespace link
} // namespace mediaservo

#endif /* MEDIASERVO_LINK_HPP */
