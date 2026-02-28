use semver::{Version, VersionReq};

fn main() {
    let v = Version::parse("9.3.0-SNAPSHOT").unwrap();
    let req = VersionReq::parse(">= 0.9.0").unwrap();
    println!("Matches: {}", req.matches(&v));
}
