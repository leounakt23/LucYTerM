# Fuzzing

Install cargo-fuzz with `cargo install cargo-fuzz`, then run targets from this
directory with `cargo fuzz run <target>`. Fuzzing is intentionally isolated
from the workspace because cargo-fuzz uses a nightly compiler and its own
profile.

The first targets cover security-sensitive pure parsers. Add VTE, Telnet, and
macro parser targets alongside them as those parser APIs evolve; all targets
must assert no panic and must not print input buffers.
