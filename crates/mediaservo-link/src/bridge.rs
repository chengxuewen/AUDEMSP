//! ROS 节点桥接配置生成（Task B3）。
//!
//! `host init` 把相机/流清单 + 令牌路径导出为 `ros_bridge.yaml`，ROS 节点
//! （stitch/vision）按此文件订阅 FrameBus topics —— 配置单一来源，杜绝 ROS
//! 侧手写 topic 名/令牌路径（spec D-H7/D-H14 增强点 2）。

/// 生成 ros_bridge.yaml 文本（手工拼接，结构固定，不引入 yaml 依赖）。
///
/// topic 命名固定：`camera/<camera-id>`、`vision/<camera-id>`（镜像相机 id）、
/// `stream/<stream-id>`。`token_path` 原样写入（调用方负责绝对路径）。
pub fn ros_bridge(cameras: &[String], streams: &[String], token_path: &str) -> String {
    let mut out = String::from("# 由 host init 生成 — ROS 节点配置单一来源（勿手改）\n");
    out.push_str(&format!("token_path: {token_path}\n"));
    out.push_str("topics:\n");
    push_section(&mut out, "camera", cameras.iter().map(|id| format!("camera/{id}")));
    push_section(&mut out, "vision", cameras.iter().map(|id| format!("vision/{id}")));
    push_section(&mut out, "stream", streams.iter().map(|id| format!("stream/{id}")));
    out
}

fn push_section(out: &mut String, name: &str, topics: impl Iterator<Item = String>) {
    out.push_str(&format!("  {name}:\n"));
    for topic in topics {
        out.push_str(&format!("    - {topic}\n"));
    }
}
