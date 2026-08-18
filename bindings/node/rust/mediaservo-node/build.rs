fn main() {
    napi_build::setup();
    // FFmpeg 动态库链接补齐（deck-c 同款 NEEDED 对齐——ffmpeg-the-third 标志在
    // 跨 crate 链上传播不完整，napi 产物曾仅 libavdevice 导致 Protocol not found）。
    // pixi 环境: PIXI_PROJECT_ROOT/.pixi/envs/default/lib（conda FFmpeg 9.0）
    if let Ok(root) = std::env::var("PIXI_PROJECT_ROOT") {
        let lib_dir = std::path::Path::new(&root).join(".pixi/envs/default/lib");
        if lib_dir.exists() {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            for lib in ["avformat", "avcodec", "avutil", "avdevice", "swscale", "swresample"] {
                println!("cargo:rustc-link-lib=dylib={lib}");
            }
        }
    }
}
