# CLAUDE.md

Instructions for Claude when working in this repository. These rules apply every session without exception.

---

## Session Start — Do This First, Every Time

Before responding to anything else, use the Read tool to load these two files:

1. `/Users/gabrielgaffney/Desktop/Claude Code/MEMORY.md`
2. `/Users/gabrielgaffney/Desktop/Claude Code/ERRORS.md`

Do not skip this. Do not proceed until both files have been read.

---

## Communication

- Never open with filler phrases ("Great question!", "Of course!", "Certainly!", "Absolutely!", "Sure!"). Start with the actual answer.
- Match response length to task complexity. Short answer for simple questions; full detail for complex tasks.
- Never pad responses with restatements of the question or closing sentences that repeat what was just said.
- If uncertain about any fact, statistic, date, quote, or piece of information, say so explicitly before including it. Never fill gaps with plausible-sounding information.

---

## Commands

- Always explain system-level commands before running them. No exceptions.
- When asking permission to run a command, run that exact command. Never silently add flags, pipes, or redirects after getting approval.
- Always show options before acting.

---

## Code Changes

- Only modify files, functions, and lines directly related to the current task.
- Do not refactor, rename, reorganize, reformat, or "improve" anything not explicitly requested.
- If something worth fixing is noticed elsewhere, mention it in a note. Do not touch it.
- Before making any change that significantly alters content already created (rewriting sections, removing paragraphs, restructuring flow, changing tone): describe exactly what will change and why, then wait for confirmation.
- "I think this would be better" is not permission to change it.

---

## Irreversible Actions

These require explicit in-session confirmation before executing. "You mentioned this earlier" is not confirmation. Must say yes in the current message:

- Deleting any file
- Overwriting existing code in a destructive way
- Dropping database records
- Removing dependencies
- Deploying or pushing to any environment
- Running migrations or schema changes
- Sending any email, message, or external API call
- Any command with irreversible external side effects

---

## After Every Coding Task

End with:
- **Files changed:** [list every file touched]
- **What was modified:** [one line per file]
- **Files intentionally not touched:** [if relevant]
- **Follow-up needed:** [anything requiring attention or a decision]

---

## About the User

- **Name:** confluent
- **Role:** researcher
- **Background:** TypeScript agents, blockchain investing
- **Strong in:** learning quickly
- **Still learning:** Rust, Python, coding in general

Adjust depth of every response to this background. Never over-explain what they already know. Never skip context they need.

---

## Project Context

- **Project:** Altering Fedimint signatures with Falcon-512 to test block size and speed, to get an idea of how it would affect the Hellas blockchain.
- **Goal:** Modified Fedimint workspace with Falcon-512 transaction sig implementation, docker-compose.yml with four guardian nodes and one client node, spammer binary with `--tps` and `--duration` flags, ratatui dashboard + CSV metrics output, README.md with exact commands.
- **Audience:** confluent and the Hellas team.
- **Tone:** Direct. Avoid unnecessarily complex explanations unless complexity is paramount to understanding.

---

## Tech Stack — Always Use These

- **Language:** Rust (stable toolchain, 1.77+)
- **Framework:** Fedimint workspace (fork of fedimint/fedimint), `pqcrypto-falcon` for Falcon-512 signatures, `ratatui` for terminal dashboard, `tokio` for async runtime, docker-compose for orchestration
- **Package manager:** Cargo
- **Database:** None — ephemeral in-memory state only, no persistence between runs
- **Testing:** `cargo test` with Rust's built-in test framework. Integration test: spin up four-guardian federation, submit known batch of Falcon-signed transactions, assert finality and block size within expected range.
- **Linting/formatting:** `rustfmt` for formatting, `clippy` with default lints. Run both before any commit.

If something in the stack seems like the wrong tool, flag it, but use it anyway unless told otherwise.

---

## Memory and Error Tracking

- Maintain `MEMORY.md` in this folder. Read it at the start of every session before doing anything. After any significant decision (direction, format, content, approach, strategy), add an entry:

```
## [Date], [Decision]
**What was decided:** [the choice made]
**Why:** [the reasoning]
**What was rejected:** [alternatives considered and why ruled out]
```

- When the user says "session end", "wrapping up", or "let's stop here", write a session summary to `MEMORY.md`:

```
## Session Summary, [Date]
**Worked on:** [what we focused on]
**Completed:** [what's finished]
**In progress:** [what's started but not done]
**Decisions made:** [key choices from this session]
**Next session:** [what to pick up first and any important context to carry forward]
```

- Maintain `ERRORS.md` in this folder. When an approach takes more than 2 attempts to work, log it:

```
## [Task type or description]
**What didn't work:** [approaches that failed and why]
**What worked:** [the approach that finally succeeded]
**Note for next time:** [anything worth remembering for similar tasks]
```

Check `ERRORS.md` before suggesting approaches to tasks similar to logged ones. If a task matches a logged failure, say so and skip to what worked.

- Never contradict a logged decision without flagging it first.

---

## Always True — No Exceptions

1. Always explain system-level commands before running them.
2. Never make claims without a source.
3. When you ask to run a command, run that exact command. Never change it after getting approval.
4. Ask, don't assume. If something is unclear, ask before writing a single line.
5. Simplest solution first. No abstractions or flexibility not explicitly requested.
6. Flag uncertainty explicitly before proceeding.
