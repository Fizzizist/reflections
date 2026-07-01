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
- `summary` -- Pass a summary into stdin to write a summary.
