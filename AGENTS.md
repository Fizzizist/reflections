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
- Always run `cargo clippy -- -D warnings` before committing code. CI treats warnings as errors; the same standard applies locally.
- Keep code comments to a minimum. Only comment in cases where something is unable to be gleaned from the code itself.
- The use of `eprintln!`, `println!`, `eprint!`, `print!`, and `dbg!` is forbidden in `src/`. All diagnostic output must go through a `logging` module. Direct stdout/stderr writes corrupt the TUI. Genuinely interactive prompts or end-of-process user-facing notices may use `write!(io::stderr(), ...)` and must include a comment justifying why.

### Testing

- Frontend ratatui testing should be done with insta snapshots.
- It is expected that Test Driven Development will be the main way that code is implemented in this repo, so most code should have tests that test _behavior_.
- Any bug fix should include regression at least one regression test.

## Build & Development Commands

```bash
cargo build                          # Build the project
cargo run                            # Run the TUI (binary name is `reflect`)
cargo test                           # Run all tests
cargo test <test_name>               # Run a single test
cargo insta review                   # Review/accept snapshot test changes
cargo clippy -- -D warnings          # Lint (CI treats warnings as errors)
cargo fmt                            # Format
```

## Architecture

The Reflections application is opinionated about keeping layers and abstractions. There is a TUI
frontend and a CLI frontend, both of which should only interact with the outer service layer. The services contain the application logic and keep state via the
repositories. Repositories are the abstraction layer around the database. The database is an
embedded Turso (libSQL) instance stored locally at `reflections.db`. Schema definitions live in
the `schema` module and are applied at startup.

### Models

The `models` module defines plain domain structs and their associated enums (e.g. `TodoItem`,
`TodoStatus`, `Meeting`, `Event`, `EventType`, `Reflection`, `Note`, `Summary`, `Tag`). Models are the in-memory representation of database rows
and carry no business logic themselves. Repositories translate between these model structs and
the database tables. Models that participate in the editor workflow implement `EditableEntityRecord`
(defined in `services::editable`), exposing `id` and `file_path`.

### TUI

Inside the `tui` module is all of the ratatui widgets for display. The root struct is App, which
takes a single Turso connection and a root directory path. It clones the connection internally to
construct all services. No application logic lives inside the tui, it is just built to display. Any
action performed by a user in the TUI is routed to a service to actually enact it. The `tui` module
includes reusable sub-components such as `InputBox` (text input with cursor, character filtering,
and max length), modal widgets (`InputModal`, `MeetingModal`, `StatusModal`), the `editor` module
(spawning `$EDITOR` on reflection/note files, suspending and restoring the terminal), and
`ReflectionsView` (listing reflections with resolved labels), and `TimelineView` (full-view-replacement
entity timeline rendered as markdown via `the-other-tui-markdown`), and `SummaryView` (
full-view-replacement summary rendered as markdown vie `the-other-tui-markdown`). Views that can create reflections and
notes (`TodoListView`, `MeetingsView`) own their own `ReflectionService` and `NoteService` clones.
`TodoListView` and `MeetingsView` also own a `TimelineService` and `Option<TimelineView>` — pressing
`Enter` on a selected item opens the timeline, which displays all events related to that entity (direct
events, linked reflections, linked notes) ordered oldest→newest, with scroll via `Ctrl+u`/`Ctrl+d` and
exit via `Esc`/`q`. A single `EditorFn` (an `Arc<dyn Fn>`) is shared across App and all views. The generic
`create_and_edit` function in `editor.rs` orchestrates the create→edit→cleanup workflow over any
`T: EditableEntity`. It is used by the `r`/`R` keys for reflections and the `n`/`N` keys for notes.
`n` in `TodoListView` and `MeetingsView` creates a note linked to the selected item; `N` in App
creates a general (unlinked) note.

### CLI

The `cli` module is for performing 1-time cli commands to services. A cli command should instantiate
and enact actions on services, similar to the tui except it is interacting with the command rather
that the tui interface. The `reflect timeline` command queries events within a time period, resolves
each event to its full entity data, reads `.md` file content for entities with a `file_path`, and
outputs JSON to stdout. Three invocation modes: `today`, `week` (Monday–Sunday), and explicit date
range. The `reflect summary create <start> <end>` command reads markdown content from stdin, creates
a summary via `SummaryService` (persisting a `SUMMARY_CREATED` event), writes the content to a
date-based `.md` file, and outputs the resulting `Summary` as JSON. The `reflect meeting sync <start> <end>`
command fetches meetings from a `CalendarBackend` (Google Calendar), upserts them into the meetings
table via `MeetingService::sync_meetings`, and outputs created/updated meetings as JSON. Timestamp
parsing and time range resolution are shared across all CLI commands via the `cli::date` module
(`parse_date_input`, `parse_date_range`, `resolve_time_range_from_args`). When no subcommand is given,
the TUI launches.

### Service

Services hold a clone of the database connection and perform operations across repositories.
If database operations are involved, the service function should open a transaction and perform.
All of the repository-related steps before committing it. All application logic should live in the
service layer. Services that participate in the editor workflow implement `EditableEntity` (defined
in `services::editable`), providing `create`, `full_path`, and `cleanup` operations. `SummaryService`
does not implement `EditableEntity` — summaries are created with content provided directly via stdin,
not through the editor workflow. The `services::tag` module provides `extract_tags` (a pure regex-based
function that pulls `#hashtag` labels from markdown content) and `sync_tags` (batched upsert + link
within a transaction). `sync_tags_from_file` reads a file, extracts tags, and syncs them — used by
both `ReflectionService::cleanup_reflection` and `NoteService::cleanup_note` in their non-empty branches.
`SummaryService::create_summary` calls `extract_tags` and `sync_tags` directly within its transaction
using in-memory content. Tags are case-insensitive (stored lowercase) and deduplicated.
`TimelineService::get_entity_timeline` gathers all events for a given entity (direct events, linked
reflections via `about_id`, linked notes via `related_to_id`), merges and sorts them oldest-first,
and resolves entity data with file content for reflections and notes.

### Repositories

Repositories are responsible for carrying out database operations on a specific entity. It is the main
translation layer between the model structs and the database tables. The
repository module for an entity should contain the SQL statements that get sent to the database.
Repository `insert` functions accept the entity UUID as a parameter (generated by the calling service),
ensuring the database entity ID matches the file name UUID. Filter-based query functions
(`EventFilter`, `ReflectionFilter`, `NoteFilter`, `MeetingFilter`) provide `find`/`find_one` with
builder-pattern filters; existing specialized functions delegate to these for API consistency.

### Schema

The `schema` module contains all DDL statements (`CREATE TABLE IF NOT EXISTS ...`) and exposes
an `init_schema` function that is called at startup to ensure the database schema exists. All table
definitions are centralized here; repositories should not create or alter tables.

### Calendar

The `calendar` module provides the abstraction layer for external calendar integration. The
`CalendarBackend` trait (in `calendar::backend`) defines the async interface for fetching meetings
within a time range. The `calendar::google` submodule implements `CalendarBackend` for Google
Calendar, handling OAuth2 authentication (browser-based flow with manual copy-paste fallback), token
storage at `~/.config/reflections/token.json`, and the Calendar API HTTP client. OAuth client
credentials are read from a `google_secrets.json` file at the project root by `build.rs` and
embedded into the binary via `cargo:rustc-env` (`GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`).
The `calendar::types` module defines the `CalendarEvent` struct shared between backends and services.
`MeetingService::sync_meetings` accepts a `&dyn CalendarBackend`, fetches events, and upserts them
(matching by name within the time range) in a single transaction.

