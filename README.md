# Reflections

![logo](./assets/reflections.png)

You ever ask yourself: why don't I have a totally overkill work journalling app that runs in my 
terminal? Well -- here's your answer to that.

## Installation

```sh
cargo install --git https://github.com/fizzizist/reflections.git
```

## Setup

If you don't use Neovim you might want to set the `EDITOR` env var so that the app knows what editor
to open when you make a note.

## Controls

Running `reflect` with no arguments bring you into the TUI.

- `r` -- Write a reflection about selected entity.
- `R` -- Write a reflection not tied to anything.
- `n` -- Write a note about a selected entity.
- `N` -- Write a note not tied to anything in particular.
- `a` -- add new item (todo item, meetings).
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

## Tags

Any `#hashtag` found in the content of reflections, notes, and summaries is automatically extracted
and linked to the entity. Tags are case-insensitive and stored lowercase. Markdown headings (`# Heading`)
are not extracted as tags — only hashtags that start with a letter (`#alpha`, `#beta-project`) are
captured. Tags can appear anywhere in the text: after whitespace, punctuation, or at the start of a
line.
