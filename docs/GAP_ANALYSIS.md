# Gap Analysis: Mascord

## Bugs
- (No active bugs) - Resolved dependency conflict in songbird features.

## Missing Features / Improvements
- [ ] **Persistent LLM Context**: Currently context is per-command and not saved across `/chat` calls.
- [ ] **Background Indexing**: Messages are saved but embeddings are only generated for the search query, not for every incoming message.
- [ ] **Vector Extension**: Automatic downloading/loading of `sqlite-vec` shared library for different OS environments.
- [ ] **Queue UI**: Improved `/queue` command and interactive playback controls (buttons/select menus).
