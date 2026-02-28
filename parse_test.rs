extern crate pulldown_cmark;
use pulldown_cmark::{html, Options, Parser};

fn main() {
    let markdown_content = std::fs::read_to_string("test.md").unwrap();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_GFM); // GitHub Flavored Markdown
    
    let parser = Parser::new_ext(&markdown_content, options);
    let mut html_content = String::new();
    html::push_html(&mut html_content, parser);
    println!("{}", html_content);
}
