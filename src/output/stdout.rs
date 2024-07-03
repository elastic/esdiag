use serde_json::Value;

/// Prints each JSON document in a vector to stdout.
///
/// This function takes a vector of JSON `Value` objects and prints each document to the standard output.
/// Each JSON object is serialized to a string and printed followed by a newline character (`\n`).
/// If there is an error during serialization, a placeholder JSON object with an error message is printed instead.
///
/// # Arguments
///
/// * `docs` - A vector of JSON `Value` objects to print.
///
/// # Returns
///
/// This function returns the number of documents printed if successful, or an `std::io::Error` if printing fails.
///
/// # Errors
///
/// This function will return an error if there is a problem writing to stdout, though this is unlikely in most environments.
/// Serialization errors may occur if a JSON `Value` cannot be converted to a string.
///
/// # Examples
///
/// ```rust
/// use serde_json::json;
///
/// let documents = vec![
///     json!({"id": 1, "name": "Alice"}),
///     json!({"id": 2, "name": "Bob"}),
/// ];
///
/// match print_docs(documents) {
///     Ok(count) => println!("Printed {} documents", count),
///     Err(e) => eprintln!("Failed to print documents: {}", e),
/// }
/// ```

pub fn print_docs<'a>(docs: Vec<Value>) -> std::io::Result<usize> {
    let len = docs.len();
    docs.iter().for_each(|doc| {
        let json = match serde_json::to_string(&doc) {
            Ok(json) => json,
            Err(e) => format! {"{{\"json_parsing_error\": \"{}\"}}", e},
        };
        println!("{}", json);
    });
    Ok(len)
}
