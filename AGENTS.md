# CLAUDE.md

This file provides guidance to LLM agents when working with code in this repository.

## Engineering Policies

These are _strict_ policies that must be followed by all engineers and developers in this project. MRs/PRs will be rejected if these policies are violated.

### Dependency Management

- All dependencies _must_ be added, removed and updated using `cargo` on the command line.
- Under no circumstances should the Cargo.toml be manually edited with regard to dependencies.

### Coding

- The use of `.unwrap()` is forbidden under _all_ circumstances. The program should _never_ panic.
- In a case where something needs to be unwrapped and it is _logically impossible_ for a panic to occur, the use of `.expect()` with an informative message is permitted.
- The use of `pub(crate)` is forbidden. It is a leaky boundary that exposes implementation details to the rest of the crate without forcing a real API decision. If something needs to cross a module boundary, either make it fully `pub` with a thought-out interface, or restructure so the caller doesn't need access at all.
- Always run `cargo fmt` before committing code.
- Always run `cargo clippy` before committing code.
- Keep code comments to a minimum. Only comment in cases where something is unable to be gleaned from the code itself.
- The use of `eprintln!`, `println!`, `eprint!`, `print!`, and `dbg!` is forbidden in `src/`. All diagnostic output must go through the `logging` module. Direct stdout/stderr writes corrupt the TUI. Genuinely interactive prompts or end-of-process user-facing notices (e.g. single-shot TTY confirmation, session-id epilogue) may use `write!(io::stderr(), ...)` and must include a comment justifying why.

### Testing

- Frontend ratatui testing should be done with insta snapshots.
- It is expected that Test Driven Development will be the main way that code is implemented in this repo, so most code should have tests that test _behavior_.
- Any bug fix should include regression at least one regression test.

## Build & Development Commands

```bash
cargo build                          # Build the project
cargo run                            # Run in REPL mode
cargo run -- --single-shot "prompt"  # Run in single-shot mode
cargo run -- --debug                 # Run with file logging (illustrious-manager_<ts>.log)
cargo run -- --config <path>         # Use a custom config file
cargo run -- --session-id <uuid>     # Resume a previous session by UUIDv7
cargo test                           # Run all tests
cargo test <test_name>               # Run a single test
cargo insta review                   # Review/accept snapshot test changes
cargo clippy                         # Lint
cargo fmt                            # Format
```

## Architecture

The Reflections application is opinionated about keeping layers and abstractions. There is a TUI and a
CLI frontend, that should only interact with the outer service layer. The services contain the 
application logic and keep state via the repositories. Repositories are the abstraction layer around the 
database.

### TUI

Inside the `tui` module is all of the ratatui widgets for display. The root struct is App, which
is fed a list of services in order to enact application logic. No application logic lives inside
the tui, it is just build to display. Any action performed by a user in the TUI is routed to a 
service to actually enact it.

### CLI

The `cli` module is for performing 1-time cli commands to services. A cli command should instantiate
and enact actions on services, similar to the tui except it is interacting with the command rather
that the tui interface.

### Service

Services hold a clone of the database connection and perform operations across repositories. 
If database operations are involved, the service function should open a transaction and perform.
All of the repository-related steps before committing it. All application logic should live in the
service layer.

### Repositories

Repositories are responsible for carrying out database operations on a specific entity. It is the main
translation layer between the model structs and the database tables. The 
repository module for an entity should contain the SQL statements that get sent to the database.

