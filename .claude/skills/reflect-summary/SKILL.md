---
name: reflect-summary
description: "Generate a summary of a timeline pulled from the reflections journalling app."
argument-hint: "<FIRST> [SECOND]"
---

# reflect-summary -- Generate a summary

## Arguments

User must provide at least `FIRST` but my provide 2 arguments. These are to be passed verbatim into the `reflect timeline` command when you do the pull.
If they are `FIRST` _and_ `SECOND` are provided, they will also be passed into the `reflect summary create` command as `<START>` and `<END>` -- otherwise that command will have to be gleaned.

## Step 1: Pull the Timeline

Use the `reflect timeline <FIRST> [SECOND]` command to pull the timeline for the user.

## Step 2: Post a Summary

Summarize the timeline emphasizing the user's accomplishments and the important business decisions. Post the summary to
the reflect tool like `echo <summary> | reflect summary create <START> <END>` -- where `START` and `END` represent the time
period that is being summarized. The user may provide the time period specifically. If only `FIRST` was provided, then
use the timestamps from the `reflect timeline` output.

The summary should take the following format:
```
# Generated Summary <START> to <END>

## Accomplishments

<list of accomplishments>

## Business Decisions

<list of business decisions>

## Other Notes

<list of things that don't necessarily fall in the 2 categories above, but are noteworthy>
```
