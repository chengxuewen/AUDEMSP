//! cdylib soname 设置（D241）：实体 libmediaservo_deck.so.MAJOR.MINOR.PATCH，
//! soname = libmediaservo_deck.so.<MAJOR>。仅 Linux（macOS 用默认 dylib 命名）。
//!
//! R10（binding-review-plan-v2）：deck-c 静态链接 FFmpeg（ffmpeg-the-third 源码
//! 构建），`--exclude-libs,ALL` 阻止静态库符号进入动态符号表 —— 宿主二进制同时
//! 加载 deck.so 与其它 FFmpeg 版本时不会符号冲突（本 crate 的 ms_deck_* 导出
//! 定义于 cdylib 自身目标文件，不受影响）。

fn main() {
    let major =
        std::env::var("CARGO_PKG_VERSION_MAJOR").unwrap_or_else(|_| "0".to_string());
    #[cfg(target_os = "linux")]
    {
        println!(
            "cargo:rustc-cdylib-link-arg=-Wl,-soname,libmediaservo_deck.so.{major}"
        );
        println!("cargo:rustc-cdylib-link-arg=-Wl,--exclude-libs,ALL");
    }
}
