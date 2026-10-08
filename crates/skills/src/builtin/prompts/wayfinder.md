# Wayfinder

Plan a huge chunk of work (more than one agent session can hold) as a shared map of decision tickets on your issue tracker, and resolve them one at a time until the way to the destination is clear.

## Plan, don't do

Wayfinder is **planning** by default: each ticket resolves a decision, and the map is done when the way is clear. Produce decisions, not deliverables.

## The Map

A single issue on the tracker, labelled `wayfinder:map`. Its tickets are child issues.

### Map body structure

```
## Destination
<what reaching the end looks like>

## Notes
<domain; skills every session should consult; standing preferences>

## Decisions so far
- [<closed ticket title>](link): <one-line gist>

## Not yet specified
<in-scope fog you can't ticket yet>

## Out of scope
<work ruled beyond the destination>
```

## Ticket Types

- **Research** (AFK): Reading docs/APIs to surface facts. Resolved by subagent.
- **Prototype** (HITL): Make a cheap artifact to react to. Links prototype as asset.
- **Grilling** (HITL): Conversation. The default case.
- **Task** (HITL or AFK): Manual work that must happen before a decision can be made.

## Fog of war

The map is deliberately incomplete. Beyond live tickets lies the **fog of war**: decisions you can tell are coming but can't pin down yet.

- **Ticket when** the question is already sharp
- **Not yet specified when** you can't phrase it sharply yet

## Invocation: Chart the map

1. Name the destination (grilling + domain-modeling)
2. Map the frontier (breadth-first grilling)
3. Create the map issue
4. Create tickets you can specify now
5. Fire research subagents for research tickets
6. Stop: charting is one session's work

## Invocation: Work through the map

1. Load the map
2. Choose the ticket (first frontier ticket in order)
3. Claim it (assign to yourself)
4. Resolve it
5. Record resolution: post answer, close issue, append to map's Decisions-so-far
6. Add newly-surfaced tickets, graduate fog

## Rules

- Never resolve more than one ticket per session (except research tickets in parallel)
- Refer to tickets by name, never by bare id/number
- Only a tracker that lacks native blocking falls back to body convention
