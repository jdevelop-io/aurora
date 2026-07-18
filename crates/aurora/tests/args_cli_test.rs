//! Positional-argument CLI tests for the old `${arg.N}`/`${args}` syntax were
//! removed here: that machinery (`resolve_arguments`, `Beam.args`) was deleted
//! in favor of beam params (`param "..." {}`, `${param.<name>}`). The
//! param-based equivalents of these tests are added by the task that wires up
//! the full params CLI surface (positional/named binding, `--list` signatures).
