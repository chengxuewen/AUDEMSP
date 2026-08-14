//! Task 3: 能力令牌测试（D238：Ed25519 签发/验签，ACL claims）。

use mediaservo_link::{
    CapabilityToken, Ed25519SigningKey, Ed25519VerifyingKey, FrameTopic, NodeAcl, NodeId, Role,
};

// 测试用 Ed25519 密钥对（openssl 生成，仅测试用）。
const PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIObCg8b+Le6kKOI/+pE+4+YhXUlr6X6h7q8p/MjvHmXT\n-----END PRIVATE KEY-----\n";
const PUB_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAgXprEbnahCZoZtLpiUqR0ruqtzEfRXk/Gl/6F6PEm4o=\n-----END PUBLIC KEY-----\n";

fn keys() -> (Ed25519SigningKey, Ed25519VerifyingKey) {
    (
        Ed25519SigningKey::from_pem(PRIV_PEM.as_bytes()),
        Ed25519VerifyingKey::from_pem(PUB_PEM.as_bytes()),
    )
}

#[test]
fn sign_verify_roundtrip_ed25519() {
    let (sk, vk) = keys();
    let acl = NodeAcl::for_role(NodeId::new("ros-stitcher"), Role::Processor);
    let tok = CapabilityToken::sign(&acl, 3600, &sk).unwrap();
    let claims = tok.verify(&vk).unwrap();
    assert_eq!(claims.role, Role::Processor);
    assert_eq!(claims.node_id, "ros-stitcher");
    // ACL 签进令牌，验签后应可用（派生 topic 权限，D239）
    assert!(claims.acl.can_publish(&FrameTopic::new("video/stitched")));
    assert!(claims.acl.can_subscribe(&FrameTopic::new("camera/front/raw")));
}

#[test]
fn tampered_token_rejected() {
    let (sk, vk) = keys();
    let acl = NodeAcl::for_role(NodeId::new("capture-0"), Role::Capture);
    let tok = CapabilityToken::sign(&acl, 3600, &sk).unwrap();
    // 篡改 payload 中间一个字节（破坏签名）
    let mut bytes = tok.as_str().as_bytes().to_vec();
    let first_dot = tok.as_str().find('.').unwrap();
    let rest = &tok.as_str()[first_dot + 1..];
    let payload_pos = first_dot + 1 + rest.len() / 2;
    bytes[payload_pos] ^= 0x01; // 翻最低位, 保持 ASCII/UTF-8 合法
    let tampered = CapabilityToken::from_raw(String::from_utf8(bytes).unwrap());
    assert!(tampered.verify(&vk).is_err(), "篡改后验签必须失败");
}

#[test]
fn expired_token_rejected() {
    let (sk, vk) = keys();
    let acl = NodeAcl::for_role(NodeId::new("capture-0"), Role::Capture);
    // ttl=0 → exp=now → 立即过期
    let tok = CapabilityToken::sign(&acl, 0, &sk).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert!(tok.verify(&vk).is_err(), "过期令牌验签必须失败");
}

#[test]
fn wrong_verifying_key_rejected() {
    let (sk, _) = keys();
    let acl = NodeAcl::for_role(NodeId::new("capture-0"), Role::Capture);
    let tok = CapabilityToken::sign(&acl, 3600, &sk).unwrap();
    // 用一个不相关的公钥验签（openssl 再生成的对，此处用篡改后的公钥模拟）
    let wrong_pem = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAkCGEQfZ6DyEUyzKgaQSvGbABtQs/W9ghbNgT0DLl9pY=\n-----END PUBLIC KEY-----\n";
    let wrong_vk = Ed25519VerifyingKey::from_pem(wrong_pem.as_bytes());
    assert!(tok.verify(&wrong_vk).is_err(), "错误公钥验签必须失败");
}
