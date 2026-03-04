# Contributing to ChronoForth

## Code Review Checklist

- [ ] Functionality works as intended
- [ ] Optimized for 6502 cycle efficiency
- [ ] Stack effects documented accurately
- [ ] Test suite passes

## Development Setup

```bash
# Required tools
# - ACME cross-assembler (v0.97+)
# - VICE C64 emulator
# - c1541 disk utility
# - make

# Build and test
make chronoforth.d64
make deploy
```

## Code Style

Formatting rules are defined in [.editorconfig](.editorconfig) and are the
canonical source of truth for this repository.

## CI / testing

Pull requests must pass the GitHub Actions workflow in
[.github/workflows/ci.yml](.github/workflows/ci.yml).

---

*Standardized with chronoboiler v1.0.0*
