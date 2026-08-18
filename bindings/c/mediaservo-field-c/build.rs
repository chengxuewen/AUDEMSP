//! cdylib soname 设置（D241）：实体 libmediaservo_<sdk>.so.MAJOR.MINOR.PATCH，
//! soname = libmediaservo_<sdk>.so.<MAJOR>。仅 Linux（macOS 用默认 dylib 命名）。

fn main() {
    let major =
        std::env::var("CARGO_PKG_VERSION_MAJOR").unwrap_or_else(|_| "0".to_string());
    #[cfg(target_os = "linux")]
    {
        println!(
            "cargo:rustc-cdylib-link-arg=-Wl,-soname,libmediaservo_field.so.{major}"
        );
    }
}
