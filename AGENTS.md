# AGENTS.md

## Philosophy

This project values domain adequacy.

Keep what the domain requires. Remove what the domain does not justify.

The code should feel small, direct, and deliberate, but not underbuilt. Simplicity is only good when it preserves correctness, clarity, and the shape of the problem.

## Principles

### Adequacy Over Minimality

Do not optimize for fewer lines, fewer files, or fewer concepts by itself.

Optimize for the right amount of structure: enough to express the domain clearly, not enough to become ceremony.

### Ownership as Design

Use Rust ownership to clarify responsibility.

State should have a natural owner. Mutation should happen where that ownership belongs. Sharing should be a domain decision, not a workaround.

### Honest Boundaries

External boundaries should be explicit and careful.

Internal code should not be filled with redundant defensive behavior that hides broken assumptions. A good boundary makes the inside simpler.

### Domain Before Mechanism

Let the problem domain shape the code.

Transport, storage, rendering, protocols, and libraries are mechanisms. They should support the domain model, not dominate it.

### Best Practices Without Ceremony

Use tests, docs, errors, logging, CI, and abstractions when they make the project more reliable or understandable.

Do not add practices only because they look professional. Good engineering should reduce noise, not create it.

## Refactoring

Look for changes that make ownership clearer, boundaries stronger, state space smaller, or domain meaning more explicit.

Prefer small, reviewable improvements. Do not rewrite broadly just because something can be made more elegant.

## Ethos

This project should feel like a small, typed, ownership-driven real-time system.

Guiding sentence:

> Keep what is adequate, remove what is unjustified, give state a clear owner, make boundaries honest, and let Rust express the shape of the domain.
