use semver::{Version, VersionReq};

fn main() {
    let v_snap = Version::parse("9.3.0-SNAPSHOT").unwrap();
    let req = VersionReq::parse(">= 0.9.0").unwrap();
    println!("Matches snapshot: {}", req.matches(&v_snap));
    
    // strip prerelease to see if it matches
    let mut v_clean = v_snap.clone();
    v_clean.pre = semver::Prerelease::EMPTY;
    println!("Matches clean: {}", req.matches(&v_clean));
}
