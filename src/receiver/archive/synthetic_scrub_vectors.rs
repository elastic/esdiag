// Hand-authored scrub test vectors only. Do NOT copy values from customer diagnostics.

/// Malformed IPv4-like string used in unit/integration tests (each octet > 255).
pub const MALFORMED_IP: &str = "512.768.1024.1280";
pub const MALFORMED_IP_WITH_PORT: &str = "512.768.1024.1280:19840";
pub const NORMALIZED_IP: &str = "2.3.4.5";
pub const NORMALIZED_IP_WITH_PORT: &str = "2.3.4.5:19840";

pub const MALFORMED_IP_SECONDARY: &str = "513.769.1025.1281";
pub const NORMALIZED_IP_SECONDARY: &str = "3.4.5.6";
pub const MALFORMED_IP_SECONDARY_WITH_PORT: &str = "513.769.1025.1281:19033";
pub const NORMALIZED_IP_SECONDARY_WITH_PORT: &str = "3.4.5.6:19033";

pub const MALFORMED_HTTP_CLIENT_ID: &str = "516.772.1028.1284";
pub const NORMALIZED_HTTP_CLIENT_ID: u64 = 101_124_105;

/// RFC 5737 TEST-NET-1 address for valid pass-through cases.
pub const VALID_IP: &str = "192.0.2.50";
pub const VALID_IP_WITH_PORT: &str = "192.0.2.50:9300";

/// 19-char lowercase hex node id/name for scrub humanization tests.
pub const SYNTHETIC_HEX_NODE_ID: &str = "aaaabbbbccccddddee0";
