HTML Templates
==============

These templates power the web user interface for the `esdiag serve` command.

They are rendered at compile time by the [Askama](https://crates.io/crates/askama) crate, which uses syntax inspired by [Jinja](https://jinja.palletsprojects.com/en/stable/).

The front-end interface is powered by [Datastar](https://data-star.dev). Datastar relies on the `data-*` attributes in the `.html` tags, greatly reducing the amount of JavaScript required. It leverages long-lived connections with server-side event streams, allowing for seamless reactivity in a very small footprint. Perfect for embedding into the final Rust binary.
