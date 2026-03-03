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


### Measures

The ultimate measure of a build is *time*. Time measurements are not deterministic. They are the ground truth though, so we need to use all the tools at our disposal to correllate a change in build time with the change in code or infrastructure.
We need to combine time measurements (volatile) with the data we can collect from a build graph (non-volatile). An example of non-volatile measurement is `cargo llvm-lines`. It does not change based on which crate you're rebuilding from and such. It depends on the build profile (dev vs release).

Non-volatile measures can be gathered as an asynchronous CI step (since it doesn't matter who runs the build, the data *must* be the same). The volatile measures will be gathered locally. They are dependent on the machine (cores, mostly) and the state of the codebase.

### Wezel's approach
Wezel places emphasis on highlighting the scenarios that get executed the most often. It associates *builds* with their *scenarios* (what code gets built - tests/non tests) and *configurations* (how it is built). 

There are four faces to Wezel:
- Ligthweight agent running locally (Pheromone) - that identifies what code-changes developers make locally. 
- The dashboard (Anthill), showcasing which scenarios get executed the most often. It lets the user make the decision as to which scenarios should be tracked by..
- The backend (Burrow) - the infrastructure beneath Anthill. It ingests events from Pheromone, stores them, and serves data to the dashboard.
- The asynchronous scenario executor (provided by the client) named Forager. It runs the scenarios and gathers the measures (both volatile and non-volatile ones).

### Forager
Forager is the benchmarking arm of Wezel. It runs on dedicated hardware provisioned by the client — consistency of the machine is essential for meaningful volatile measurements. Forager does not prescribe *how* it is triggered; a cron job, a scheduled CI pipeline on a self-hosted runner, or a manual invocation all work.

#### Flow
1. The user observes in Anthill which scenarios are most common (derived from Pheromone data).
2. The user pins interesting scenarios for tracking and defines them as **mutations**: a recipe like "build the workspace clean, then add this function to this source file, then rebuild."
3. Forager runs tracked scenarios periodically (e.g. nightly) against HEAD of the main branch.
4. Each scenario is executed multiple times to establish statistical confidence — a single timing is not trustworthy even on dedicated hardware.
5. Results (wall time, peak RSS, and any non-volatile measures like `cargo llvm-lines`) are reported to Burrow.
6. Burrow compares results against a configurable baseline (previous run, rolling average, or a pinned threshold).
7. If a regression is detected, Forager bisects the commits between the last known-good run and the current HEAD to identify the culprit.

#### Bisection
Bisection is embarrassingly parallel. Each commit under test is independent, so the user can provision multiple worker machines to test commits concurrently. With enough workers, every commit in the range can be tested in a single round — no binary search needed.

The architecture is:
- **Forager orchestrator** — decides what to run, interprets results, talks to Burrow.
- **Forager workers** (N machines, same specs) — stateless. They pull jobs, run `forager measure <scenario> --at <sha>`, and report back.

Worker provisioning and scaling is the client's responsibility. Forager just needs a way to reach them (or they pull from a queue).

#### Scenario definition
A scenario is a user-defined **mutation recipe**:
1. A baseline build step (e.g. clean build of the workspace).
2. A mutation (e.g. "add this function to `src/lib.rs` in crate `foo`").
3. A rebuild step (e.g. `cargo build`).

This makes incremental build benchmarks deterministic and reproducible. The user controls exactly what "dirty" means.

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
  benchmark:
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
