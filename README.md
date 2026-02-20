# The Ludwig Editor

```text
{**********************************************************************}
{                                                                      }
{            L      U   U   DDDD   W      W  IIIII   GGGG              }
{            L      U   U   D   D   W    W     I    G                  }
{            L      U   U   D   D   W ww W     I    G   GG             }
{            L      U   U   D   D    W  W      I    G    G             }
{            LLLLL   UUU    DDDD     W  W    IIIII   GGGG              }
{                                                                      }
{**********************************************************************}
```

[![Rust](https://github.com/clstrfsck/ludwig-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/clstrfsck/ludwig-rs/actions/workflows/rust.yml)

## About

Ludwig is a text editor, originally for VAX/VMS and written at the University of
Adelaide.  The earliest date I can find in the source code change logs mentions
1979, so the editor has been around for some time.

This is a Rust rewrite of the editor.  It's currently very incomplete, but it's my
aspiration to make this a modern re-implementation of the editor, including
such things as syntax highlighting, which I don't seem to be able to manage
without these days.  If you would prefer something more complete, there are
a few options:

- The original Pascal code is available here: [cjbarter/ludwig](https://github.com/cjbarter/ludwig)
- There is also a C++ port available here: [clstrfsck/ludwig-c](https://github.com/clstrfsck/ludwig-c)
- There is a Go port available here: [clstrfsck/ludwig-go](https://github.com/clstrfsck/ludwig-go)

The Pascal port is likely the most faithful, functionality wise.  The Go port has
a few features that the others do not, including embedded help contents which
reduces the amount of mucking around required to use the help system.

## Building

I'm building everything using using `cargo` at the moment.
Using `task`:

```sh
# Build debug binary
cargo build
# Build release binary
cargo build --release
# Run unit tests
cargo test
# Build everything and run all available tests
cargo clippy
# ...and so on, and so forth...
```

## Coverage

Unit test coverage is lowish, but not terrible right now.  I usually go full
speed ahead until something trips me up, and then backfill with tests
until I uncover the problem.

This "worked on my machine", a somewhat recent Macbook Pro:

```sh
# Once-only install
cargo install cargo-llvm-cov

# Generate HTML
cargo llvm-cov --html
open target/llvm-cov/html/index.html
```

## System Tests

There is reasonable system test coverage.  The system tests leverage
Ludwig's batch mode, where a command string is provided on stdin.  The
general approach is:

- The test provides a selection of initial filenames and contents, together
  with expected output files and contents and a command string
- The test framework creates a temporary directory and populates it with the
  supplied files
- The command string is piped into a Ludwig process running in the temporary
  directory
- Once the process completes, the files in the temporary directory are
  collected and compared against expectations

You can clone the
[system tests](https://github.com/clstrfsck/ludwig-system-test) using:

```sh
git clone https://github.com/clstrfsck/ludwig-system-test system-test
pip install pytest pexpect

# Assuming you have python, pytest and pexpect installed
./system-test/run-system-tests.sh
```

The intention is that the system tests are cloned into a sub-directory of
the main project.  If you would like to arrange things differently,
you can use the environment variable `LUDWIG_EXE` to point the tests to
your executable.  Note that this path will need to be an absolute path.

Once the tests are running, you should see a bunch of dots, followed by
something like:

```text
157 failed, 238 passed, 7 skipped in 2.63s
```

This should give you an idea of the level of completeness of this
implementation.

I have checked that the system tests run as expected using the original
Pascal version as an oracle, as well as running them against this port.

## Approach

I really wanted the implementation to (reasonably) faithfully mimic the
original, at least initially.  Things that are obviously bugs I did not want
to reproduce, e.g. `0D` marking frames as modified, but quirks such as
the difference in behaviour between `>A` and `1A` on the line prior to the
`<End of File>` line have been reproduced.

Conscious decisions to diverge are mostly documented in the "Divergences"
section.

On the other hand, the internals have diverged significantly.  Rather than
an indexed skip list of lines for the contents of each frame, a
[rope](https://en.wikipedia.org/wiki/Rope_(data_structure)) is used.
Another structural difference has compiled code generated into a tree
structure and interpreted from there.

The former choice has clear advantages over the linked list approach, and
few downsides other than implementation complexity.  It's not obvious to
me that the latter choice is better in any way other than it's easier to
reason about.

## Divergences

I've made the following choices to diverge from the Pascal version.  In my
view, most of these items would likely be considered bugs if scrutinised
closely:

### Zero leading parameters

In the original, a zero leading parameter for the following commands marks
the frame as modified, even though this command is a no-op.  The Rust
version does not mark the buffer modified in these cases.

- `0*E`, `0*L`, `0*U` case change commands.
- `0C` insert character
- `0D` delete command.
- `0SW` swap / move lines.
