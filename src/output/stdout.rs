use serde_json::Value;

pub fn print_docs<'a>(docs: Vec<Value>) -> std::io::Result<usize> {
    let len = docs.len();
    docs.iter().for_each(|doc| {
        let json = match serde_json::to_string(&doc) {
            Ok(json) => json,
            Err(e) => format! {"{{\"json_parsing_error\": \"{}\"}}", e},
        };
        println!("{}\n", json);
    });
    Ok(len)
}
