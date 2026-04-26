## Wezel vision doc
The reality:
1. Companies do not care about builds - they care about their product.
2. They do not want to hire build experts or spend time refactoring their build system.
3. When they do spend time on optimizing the build system, they want to see the gains.
4. Those gains are elusive - builds cannot be "refactored". They rot quickly, because the quality of a build graph is neither a primary
  concern nor an easy one to maintain.
5. It is not easy to tell at a glance what the impact of a code change is on the build graph. As engineers, we focus on systems correctness and not on build performance. We do not have necessary tools to care about them, and there is no way to prevent future regressions.


Wezel slides into that reality by helping keep your hand on a pulse. It's a suite of tools for tracking the health of your build. It is out of your way, lightweight and uncompromising. Yet, it will ring the bells the moment
your dev experience regresses. It let's you introspect your code and how it corresponds to the impact on a total build time.


### Measurements and Summaries

The ultimate measure of a build is *time*. Time measurements are not deterministic. They are the ground truth though, so we need to use all the tools at our disposal to correlate a change in build time with the change in code or infrastructure.
We need to combine time measurements (volatile) with the data we can collect from a build graph (non-volatile). An example of non-volatile measurement is `cargo llvm-lines`. It does not change based on which crate you're rebuilding from and such. It depends on the build profile (dev vs release).

Non-volatile measures can be gathered as an asynchronous CI step (since it doesn't matter who runs the build, the data *must* be the same). The volatile measures will be gathered locally. They are dependent on the machine (cores, mostly) and the state of the codebase.

Wezel distinguishes between two levels of data:

- **Measurements** — raw observations emitted by a forager plugin. A single experiment step can produce many measurements (e.g. `cargo llvm-lines` emits one measurement per function, tagged with `function` and `unit`). Measurements carry arbitrary JSON values and are always surfaced to users for inspection.

- **Summaries** — named scalars derived from measurements via a pure aggregation function (`sum`, `mean`, `median`, `max`, or `min`), optionally filtered by tags. Summaries are defined in the experiment TOML and are the only thing used for regression detection and bisection. For example, `total-llvm-lines` might be the `sum` of all `llvm-lines` measurements where `unit=lines`.

This split keeps raw data intact for debugging while giving regression detection a clean, well-typed scalar to compare.

### Wezel's approach
Wezel places emphasis on highlighting the scenarios that get executed the most often. It associates *builds* with their *scenarios* (what code gets built - tests/non tests) and *configurations* (how it is built). 

There are four faces to Wezel:
- Ligthweight agent running locally (Pheromone) - that identifies what code-changes developers make locally. 
- The dashboard (Anthill), showcasing which scenarios get executed the most often. It lets the user make the decision as to which scenarios should be tracked by..
- The backend (Burrow) - the infrastructure beneath Anthill. It ingests events from Pheromone, stores them, and serves data to the dashboard.
- The asynchronous scenario executor (provided by the client) named Forager. It runs the scenarios and gathers the measures (both volatile and non-volatile ones).

### Plugins

Wezel is pluggable along two axes: **foragers** (`forager-<name>`) run experiment steps and emit measurements; **pheromones** (`pheromone-<build-system>`) classify shell commands into scenarios. Both follow the same plugin model.

#### Distribution and pinning

Plugins are standalone binaries distributed via GitHub releases (one asset per OS/arch per release).

Every project declares its plugins explicitly in `.wezel/plugins.toml`:

```toml
[forager.llvm-lines]
source = "github:wezel-build/forager-llvm-lines"
version = "1.2.3"

[pheromone.cargo]
source = "github:wezel-build/pheromone-cargo"
version = "0.5.0"
```

Resolved versions and content hashes are pinned in `.wezel/plugins.lock`, which is committed. Silent drift in plugin behavior between versions would corrupt historical comparisons; the lockfile is non-negotiable.

There is **no implicit PATH discovery**. A `forager-foo` binary sitting on `$PATH` will not be picked up — it must be declared in the manifest. The project's manifest plus its lockfile are the complete record of what code can influence measurements.

#### Local plugins

A plugin may be sourced from a local path:

```toml
[forager.custom-bench]
source = "path:/home/alice/dev/forager-custom-bench"

[forager.experimental]
source = "path:../sibling-repo/target/release/forager-experimental"
```

The path is unrestricted — it does not need to live inside the project tree. The point is not *where* the binary lives but that it is **explicitly declared**: nothing on `$PATH` can affect a wezel run unless the project's manifest names it. Local plugins are content-hashed and pinned in the lockfile just like remote ones; changes to the source trigger a reinstall.

#### Installation and schema cache

`wezel` resolves the manifest and installs each plugin into `~/.wezel/plugins/<name>/<version-or-hash>/`:

```
~/.wezel/plugins/pheromone-cargo/0.5.0/
  pheromone-cargo
  schema.json
```

Installation runs `<plugin> --schema` exactly once and persists the result alongside the binary. **Runtime never re-runs `--schema`.** Pheromone is invoked from a shell precmd hook and cannot afford a fork-and-exec per prompt; foragers don't strictly need the optimization but use the same code path for uniformity. New version → new directory → new schema, so cache invalidation is free.

The schema is also published as a release asset for IDE / static-tooling integrations that prefer not to execute untrusted binaries.

#### Schema and validation

`<plugin> --schema` prints a JSON Schema describing the plugin's input shape. Wezel reads the cached copy to validate `experiment.toml` (or the equivalent pheromone config) at load time, before any work runs.

JSON Schema only catches shape errors. For semantic checks (does this package exist, does this patch apply, is this build target reachable), plugins may implement a `<plugin> validate <inputs-file>` subcommand that wezel calls during the same load-time pass.

### Forager
Forager is the experimentation arm of Wezel. It runs on dedicated hardware provisioned by the client — consistency of the machine is essential for meaningful volatile measurements. Forager does not prescribe *how* it is triggered; a cron job, a scheduled CI pipeline on a self-hosted runner, or a manual invocation all work.

#### Flow
1. The user observes in Anthill which scenarios are most common (derived from Pheromone data).
2. The user pins interesting scenarios for tracking and defines them as **mutations**: a recipe like "build the workspace clean, then add this function to this source file, then rebuild."
3. Forager runs tracked scenarios periodically (e.g. nightly) against HEAD of the main branch.
4. Each scenario is executed multiple times to establish statistical confidence — a single timing is not trustworthy even on dedicated hardware.
5. Results are reported to Burrow: raw **measurements** from each step, plus **summaries** computed by aggregating those measurements according to formulas defined in the experiment TOML.
6. Burrow compares each summary against recent history. If a regression is detected in a bisect-eligible summary, Burrow enqueues a bisection.
7. Forager workers test the midpoint commits; bisection narrows until the culprit is identified.

#### Plugin invocation contract

A forager step is invoked as:

- `FORAGER_INPUTS=<path>` — JSON file containing the step's config (validated against the plugin's schema by wezel before invocation).
- `FORAGER_OUT_DIR=<dir>` — fresh per-step directory. The plugin writes:
  - `measurements.json` — the envelope of measurements (required).
  - `artifacts/...` — anything else the plugin wants to preserve (flamegraphs, raw tool output, build logs).
- **cwd is the experiment workspace** (see below).

Reserved CLI flags (`--schema`, `--version`, future `validate`) are *only* for protocol concerns. Inputs never go through argv — file-in, files-out is the contract.

On any step failure, wezel records enough to reconstruct the failing invocation (the run ID, step name, plugin version, resolved inputs, and SHA), and prints a `wezel reproduce <run>:<step>` command that re-materializes the workspace, re-writes the inputs file, and re-invokes the plugin verbatim. The user can also pass `--keep-workspace` to a forager run to skip cleanup when actively debugging.

#### Experiment workspace

Each experiment run gets its own **workspace** — a checkout of the source tree at the SHA under test. The workspace is the cwd of every step in the run and persists across all steps. Build caches (`./target`, `./node_modules`) accumulate naturally, so an experiment of the form "clean build, then apply patch and rebuild" measures incremental compile time correctly without any plugin awareness.

Workspaces are materialized via `git worktree` off a shared bare clone — fast even for hundred-commit bisections, since history is downloaded once per worker and each checkout is near-instant.

**Exception:** if the target repo contains `.gitmodules`, wezel falls back to a fresh clone per job. Submodule HEAD is shared across worktrees of the same parent repo, so concurrent bisection workers would silently corrupt each other's submodule state.

Workspaces are cleaned up unconditionally at the end of a run, on both success and failure. Preservation is opt-in (`--keep-workspace`); accumulating failed workspaces across many bisection rounds would fill the disk silently. When a step fails, the recorded run metadata is enough for `wezel reproduce` to recreate the workspace from scratch, so nothing is lost by cleaning up.

#### Bisection
Bisection is embarrassingly parallel. Each commit under test is independent, so the user can provision multiple worker machines to test commits concurrently. With enough workers, every commit in the range can be tested in a single round — no binary search needed.

The architecture is:
- **Forager orchestrator** — decides what to run, interprets results, talks to Burrow.
- **Forager workers** (N machines, same specs) — stateless. They pull jobs, run `forager measure <scenario> --at <sha>`, and report back.

Worker provisioning and scaling is the client's responsibility. Forager just needs a way to reach them (or they pull from a queue).

#### Experiment definition
An experiment lives in `.wezel/experiments/<name>/experiment.toml`. It declares:

- **Steps** — ordered list of forager plugin invocations (e.g. `llvm-lines`, `exec`). Each step produces zero or more tagged measurements.
- **Summaries** — aggregation formulas over measurements, with optional tag filters. Each summary has a name, an aggregation function (`sum`, `mean`, `median`, `max`, `min`), and a `bisect` flag controlling whether regressions in that summary trigger bisection.

```toml
[[steps]]
name = "measure-llvm"
tool = "llvm-lines"
package = "my-crate"

[[summaries]]
name = "total-llvm-lines"
measurement = "llvm-lines"
aggregation = "sum"
filter = { unit = "lines" }
bisect = true

[[summaries]]
name = "total-copies"
measurement = "llvm-lines"
aggregation = "sum"
filter = { unit = "copies" }
bisect = false   # informational only
```

Steps may also apply a patch file before running (`apply-diff = true`), enabling incremental build experiments: run a baseline, apply the patch, rebuild, compare.

#### Alerting
When Forager identifies a culprit commit, it needs to notify someone. At minimum, Forager exposes a **webhook** so users can wire it to Slack, email, GitHub comments, or whatever fits their workflow. Anthill also surfaces bisect results in the dashboard.

#### Integration example
A minimal GitHub Actions setup with a self-hosted runner:
```yaml
# .github/workflows/forager.yml
on:
  schedule:
    - cron: '0 3 * * *'
jobs:
  experiment:
    runs-on: self-hosted
    steps:
      - uses: actions/checkout@v4
      - run: forager run --report
```
The workflow file is trivial and stable. All scenario logic lives in the `forager` binary; scenario configuration lives in Burrow.

### Burrow
Burrow is Anthill's backend. It receives events flushed by Pheromone, persists them, and exposes the data Anthill needs to render scenarios, configurations, and their measures.

### Pheromone
Pheromone is an agent running locally. It consists of a single binary (pheromone_cli) that is invoked via precmd hooks in the shell. The cli delegates to the build-system-specific processes named `pheromone-<build system>` such as `pheromone-cargo` for Rust. The build system-specific process is responsible for identifying the scenario being executed and reporting it back to the pheromone_cli.
All events are dumped into ~/.wezel/events/.json. As a post-cmd hook (in the background), wezel will flush the events to the currently configured Anthill instance.

pheromone_cli is thus responsible for:
- shell handling (precmd and postcmd hooks)
- Alias normalization (cargo build and cargo b are the same)
- Flushing the events to Anthill

#### Custom toolchains
Build systems often circumvent the shell; for example, rustup may end up invoking the cargo binary directly. In such cases one can set up a custom toolchain that invokes pheromone-cargo instead of cargo (busybox-style). This way, the events will be captured as well. The same applies to other build systems. Wezel will provide a set of instructions for setting up such custom toolchains for the most popular build systems.
