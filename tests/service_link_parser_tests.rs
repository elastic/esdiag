use std::process::Command;

const SERVICE_LINK_PARSER: &str = include_str!("../templates/components/service_link_parser.js");

#[test]
fn service_link_parser_strips_quotes_from_pasted_curl_command() {
    let script = format!(
        r#"
{SERVICE_LINK_PARSER}

const assert = require("node:assert/strict");

const cases = [
    {{
        name: "escaped quotes from case comment",
        command: String.raw`curl -s -L -H \"Authorization: api_key_value\" -o 'diag_file_name.zip' \"https://hostname/path/file\"`,
        expected: {{
            token: "api_key_value",
            filename: "diag_file_name.zip",
            url: "https://hostname/path/file",
        }},
    }},
    {{
        name: "double quoted curl values",
        command: `curl -H "Authorization: token" -o "diag.zip" "https://host/path/file"`,
        expected: {{
            token: "token",
            filename: "diag.zip",
            url: "https://host/path/file",
        }},
    }},
    {{
        name: "single quoted curl values with trailing sentence period",
        command: `curl -H 'Authorization: token' --output 'diag.zip' 'https://host/path/file.'`,
        expected: {{
            token: "token",
            filename: "diag.zip",
            url: "https://host/path/file",
        }},
    }},
];

for (const testCase of cases) {{
    assert.deepEqual(
        {{
            token: parseToken(testCase.command),
            filename: parseFilename(testCase.command),
            url: parseUrl(testCase.command),
        }},
        testCase.expected,
        testCase.name,
    );
}}
"#,
    );

    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .output()
        .expect("node must be available to test service link parser");

    assert!(
        output.status.success(),
        "service link parser test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
