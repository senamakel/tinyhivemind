# Examples

Runnable, CI-compiled usage of the crate's public API. Examples cannot drift
from what the crate actually exports, because `cargo build --all-targets`
compiles them and `cargo clippy --all-targets` lints them.

## Files

- `basic.rs` — the smallest end-to-end walkthrough: the four spellings of the
  default desk (`is_general_chat`), when two chat ids name one conversation
  (`same_conversation`), and building a `DeskSet` from a declared blueprint
  plus runtime member additions and an explicit order, then reading its
  resolved membership back. Run with:

  ```sh
  cargo run -p tinyhivemind-core --example basic
  ```
