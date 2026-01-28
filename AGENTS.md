# Agent Guidelines for Mascord

## Core Principles

### SOLID
- **S**ingle Responsibility: Each module/struct handles one concern only
- **O**pen/Closed: Extend behavior through traits, not modification
- **L**iskov Substitution: Trait implementations must be interchangeable
- **I**nterface Segregation: Small, focused traits over large ones
- **D**ependency Inversion: Depend on abstractions (traits), not concretions

### KISS (Keep It Simple, Stupid)
- Prefer straightforward solutions over clever ones
- Avoid premature abstraction
- Write code that's easy to read and understand
- When in doubt, choose the simpler approach

### YAGNI (You Aren't Gonna Need It)
- Only implement what's currently needed
- Don't add features "just in case"
- Remove dead code immediately
- Avoid speculative generality

---

## Project Conventions

### Code Style
- Use `rustfmt` for formatting
- Use `clippy` for linting: `cargo clippy -- -D warnings`
- Prefer `thiserror` for custom error types
- Use `tracing` for logging, not `println!`

### Architecture Rules
1. **Commands** call into **services**, never directly to DB/external APIs
2. **Services** contain business logic
3. **Repositories** handle data persistence
4. All async code uses `tokio`

### Naming
- Modules: `snake_case`
- Types/Traits: `PascalCase`
- Functions/Variables: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`

### Error Handling
- Use `Result<T, Error>` for fallible operations
- Propagate errors with `?` operator
- Log errors at the boundary, not deep in call stack
- Provide context with `.context()` from `anyhow` or custom errors

### Testing
- Unit tests in same file: `#[cfg(test)] mod tests`
- Integration tests in `tests/` directory
- Mock external dependencies using traits

---

## Documentation Requirements

Keep these files updated after each task:
- `docs/REQUIREMENTS.md` - Functional and non-functional requirements
- `docs/ARCHITECTURE.md` - Component design and dependencies
- `docs/COMPONENT_*.md` - Domain-specific documentation
- `docs/GAP_ANALYSIS.md` - Bugs and missing features (remove when resolved)
