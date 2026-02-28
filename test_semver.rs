use semver::{Version, VersionReq};

fn main() {
    let req1 = ">= 0.9.0 < 5.1.1";
    let req2 = ">=0.9.0 <5.1.1";
    
    match VersionReq::parse(req1) {
        Ok(_) => println!("req1 parsed"),
        Err(e) => println!("req1 failed: {}", e),
    }

    match VersionReq::parse(req2) {
        Ok(_) => println!("req2 parsed"),
        Err(e) => println!("req2 failed: {}", e),
    }

    // Try replace spaces logic
    let mut parts = req1.split_whitespace().peekable();
    let mut out = String::new();
    while let Some(part) = parts.next() {
        out.push_str(part);
        if let Some(next) = parts.peek() {
            if next.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                // If next is a digit, it's a version number, don't add comma
            } else if part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                // If current is a digit, add comma before the next operator
                out.push_str(", ");
            }
        }
    }
    println!("transformed: {}", out);
    match VersionReq::parse(&out) {
        Ok(_) => println!("transformed parsed"),
        Err(e) => println!("transformed failed: {}", e),
    }
}
