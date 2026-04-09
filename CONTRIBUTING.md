# Contributing to Rapcode

First off, thank you for considering contributing to Rapcode! It's people like you that make Rapcode such a great tool.

## Where do I go from here?

If you've noticed a bug or have a feature request, make one! It's generally best if you get confirmation of your bug or approval for your feature request this way before starting to code.

## Fork & create a branch

If this is something you think you can fix, then fork Rapcode and create a branch with a descriptive name.

A good branch name would be (where issue #325 is the ticket you're working on):

```sh
git checkout -b 325-add-new-refactoring-rule
```

## Get the test suite running

Make sure you have Rust and Cargo installed.

```sh
cargo build
cargo test
```

## Implement your fix or feature

At this point, you're ready to make your changes. Feel free to ask for help; everyone is a beginner at first!

1. Code your changes.
2. Add relevant tests if possible.
3. Run `cargo clippy` and `cargo fmt` to follow Rust norms.
4. Push your branch to your fork.
5. Create a Pull Request (PR).

## Code Review

Once you've submitted a PR, the maintainers will review your changes. They may ask for modifications or clarity. We aim to keep reviews friendly and constructive.

## Code of Conduct

Please remember that all contributors are expected to uphold our [Code of Conduct](CODE_OF_CONDUCT.md). Please report unacceptable behavior to us.

Thanks again for contributing!