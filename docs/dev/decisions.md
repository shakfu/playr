# Decisions

Settled. `TODO.md` cites them by number.

1. **Settings and state.** `settings.toml` holds settings, which the user writes: how playr behaves, including whether it remembers something, as `persist` does. `library.db` holds state, which playr writes: tracks, analysis, playlists, marks, loops, the resume position, the queue and the selection, and any session value a setting says to remember. playr never writes `settings.toml`, and a user never needs to open `library.db`.

2. **The primary user** buys music rather than streams it, has eclectic tastes, and both listens to it and samples it for music production, as a hobby or semi-professionally. The player and the cutter matter equally.

3. **Exported slices serve any sampler,** not only rtrack. What an export means should reach other software in formats it reads, not only in `samples.json`.

4. **Command renames** are acceptable before 1.0, each with the owner's approval.
