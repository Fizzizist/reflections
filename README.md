# Reflections

![logo](./assets/reflections.png)

You ever ask yourself: why don't I have a totally overkill work journalling app that runs in my 
terminal? Well -- here's your answer to that.

## Installation

### Pre-compiled binaries

macOS (Apple Silicon):

```sh
brew install Fizzizist/tap/reflections-bin
```

Or download the latest tarball from the [releases page](https://github.com/Fizzizist/reflections/releases).

Linux:

Download the latest tarball from the [releases page](https://github.com/Fizzizist/reflections/releases).

### From source

```sh
cargo install --git https://github.com/fizzizist/reflections.git
```

Pre-compiled binaries include embedded Google OAuth credentials for calendar sync. Building from source requires your own `google_secrets.json` (see Setup below).

## Setup

If you don't use Neovim you might want to set the `EDITOR` env var so that the app knows what editor
to open when you make a note.

Google Calendar sync requires OAuth credentials provided at build time via a `google_secrets.json`
file at the project root containing `client_id` and `client_secret` fields. The developer (not the
end user) creates an OAuth 2.0 Desktop app client in the Google Cloud Console and places the
credentials in this file. Tokens are cached per-user at `~/.config/reflections/token.json`. See
[Google's OAuth desktop app documentation](https://developers.google.com/identity/protocols/oauth2/native-app)
for creating credentials.

## Controls

Running `reflect` with no arguments bring you into the TUI.

- `r` -- Write a reflection about selected entity.
- `R` -- Write a reflection not tied to anything.
- `n` -- Write a note about a selected entity.
- `N` -- Write a note not tied to anything in particular.
- `a` -- add new item (todo item, meetings).
- `A` -- toggle showing all todo items, including those marked DONE.
- `u` -- update the status of the selected todo item.
- `e` -- edit a summary (while viewing a summary).
- `Enter` -- view selected item (todo item, meeting, summary).
- `j`, `k`, `h`, `l` -- navigation
- `gt`, `gT` -- tab navigation

## Commands

The CLI here was mainly designed to be used by an AI to read from the timeline and produce summaries
of what the user did for that time period.

- `timeline` -- output a timeline for the given time period in JSON format
  - `reflect timeline today`
  - `reflect timeline week`
  - `reflect timeline <start> <end>` (format: `%Y-%m-%d` or `%Y-%m-%d %H:%M`)
- `summary create <start> <end>` -- create a summary by piping markdown content via stdin
  - `echo "content" | reflect summary create 2026-06-30 2026-07-06`
  - Timestamps use the same format as `timeline`
- `meeting sync <start> <end>` -- sync meetings from Google Calendar into the local database
  - `reflect meeting sync today`
  - `reflect meeting sync week`
  - `reflect meeting sync <start> <end>` (format: `%Y-%m-%d` or `%Y-%m-%d %H:%M`)
  - Meeting matching is by name within the time range; existing meetings with the same name but a different time are updated
  - Outputs created/updated meetings as JSON

## Tags

Any `#hashtag` found in the content of reflections, notes, and summaries is automatically extracted
and linked to the entity. Tags are case-insensitive and stored lowercase. Markdown headings (`# Heading`)
are not extracted as tags — only hashtags that start with a letter (`#alpha`, `#beta-project`) are
captured. Tags can appear anywhere in the text: after whitespace, punctuation, or at the start of a
line.
